//! Compute backend selection for whisper.cpp.
//!
//! The backend is fixed at build time by the `cuda`, `vulkan` and `metal`
//! Cargo features; at run time the user can only turn the GPU off or pick a
//! device. A Vulkan build also checks
//! at startup that a usable device exists, because the same binary ships to
//! machines without a GPU. The decision logic is plain data so it can be
//! tested without a GPU.

use whisper_rs::WhisperContextParameters;

/// whisper.cpp backend this binary was compiled with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Cuda,
    Vulkan,
    Metal,
}

impl Backend {
    /// GPU backend selected by Cargo features, or [`Backend::Cpu`].
    ///
    /// When several GPU features are enabled at once the first in the order
    /// CUDA, Vulkan, Metal is reported; whisper.cpp itself picks the first GPU
    /// device it registers.
    pub const fn compiled() -> Self {
        if cfg!(feature = "cuda") {
            Self::Cuda
        } else if cfg!(feature = "vulkan") {
            Self::Vulkan
        } else if cfg!(feature = "metal") {
            Self::Metal
        } else {
            Self::Cpu
        }
    }

    /// Human-readable name for logs.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Cuda => "CUDA",
            Self::Vulkan => "Vulkan",
            Self::Metal => "Metal",
        }
    }

    pub const fn is_gpu(self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

/// Why [`GpuSettings`] picked its backend; drives the startup log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A GPU backend with a usable device.
    Gpu,
    /// The binary was built without a GPU backend.
    CpuBuild,
    /// `--no-gpu` / `WHISPER_NO_GPU`, or the CPU was forced internally.
    Disabled,
    /// Built with a GPU backend, but the host has no usable device: no
    /// Vulkan loader, no driver, or only a software renderer.
    NoDevice,
    /// `--gpu-device` is beyond the devices that were found.
    NoSuchDevice { requested: i32, available: usize },
}

/// Run-time GPU choice, resolved once at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuSettings {
    /// Backend to request from whisper.cpp for this run.
    pub backend: Backend,
    /// GPU device index, for machines with more than one GPU.
    pub device: i32,
    /// Why this backend was chosen.
    pub reason: Reason,
}

impl GpuSettings {
    /// CPU decoding regardless of how the binary was built.
    pub const fn cpu() -> Self {
        Self::on_cpu(Reason::Disabled)
    }

    const fn on_cpu(reason: Reason) -> Self {
        Self {
            backend: Backend::Cpu,
            device: 0,
            reason,
        }
    }

    /// Resolves the CLI flags against the compiled backend and, when the
    /// backend can tell, the GPU devices present on this host.
    pub fn from_flags(no_gpu: bool, device: i32) -> Self {
        let compiled = Backend::compiled();
        Self::resolve(compiled, no_gpu, device, || count_devices(compiled))
    }

    /// [`GpuSettings::from_flags`] with the compiled backend and the device
    /// probe injected, so every combination is testable from a CPU-only
    /// build. The probe only runs when the GPU is actually wanted.
    fn resolve(
        compiled: Backend,
        no_gpu: bool,
        device: i32,
        devices: impl FnOnce() -> Option<usize>,
    ) -> Self {
        if !compiled.is_gpu() {
            return Self::on_cpu(Reason::CpuBuild);
        }
        if no_gpu {
            return Self::on_cpu(Reason::Disabled);
        }
        match devices() {
            Some(0) => Self::on_cpu(Reason::NoDevice),
            Some(available) if usize::try_from(device).map_or(true, |d| d >= available) => {
                Self::on_cpu(Reason::NoSuchDevice {
                    requested: device,
                    available,
                })
            }
            _ => Self {
                backend: compiled,
                device,
                reason: Reason::Gpu,
            },
        }
    }

    pub const fn uses_gpu(self) -> bool {
        self.backend.is_gpu()
    }

    /// One-line description for the startup log.
    pub fn describe(self) -> String {
        let built = Backend::compiled().name();
        match self.reason {
            Reason::Gpu => format!("{} (device {})", self.backend.name(), self.device),
            Reason::CpuBuild => "CPU (built without GPU support)".to_string(),
            Reason::Disabled => format!("CPU (GPU disabled; built with {built})"),
            Reason::NoDevice => format!("CPU (built with {built}, but no usable GPU was found)"),
            Reason::NoSuchDevice {
                requested,
                available,
            } => format!(
                "CPU (GPU device {requested} not found; {available} {built} device(s) available)"
            ),
        }
    }

    /// whisper.cpp context parameters for these settings.
    ///
    /// Flash attention is only enabled on the GPU, where it is a clear win
    /// for the turbo models; the CPU path keeps whisper.cpp's defaults so its
    /// output does not change.
    pub fn context_params(self) -> WhisperContextParameters<'static> {
        let mut params = WhisperContextParameters::default();
        params
            .use_gpu(self.uses_gpu())
            .flash_attn(self.uses_gpu())
            .gpu_device(self.device);
        params
    }
}

/// Usable GPU devices on this host, when the backend can tell before a model
/// is loaded; `None` leaves the decision to whisper.cpp.
///
/// Only Vulkan builds probe: they are the ones shipped to every Linux and
/// Windows machine, with or without a GPU. whisper.cpp ignores software
/// renderers such as llvmpipe, and the patched ggml-vulkan reports zero
/// devices when the Vulkan loader is missing (see vendor/README.md).
#[cfg(feature = "vulkan")]
fn count_devices(compiled: Backend) -> Option<usize> {
    (compiled == Backend::Vulkan).then(|| whisper_rs::vulkan::list_devices().len())
}

#[cfg(not(feature = "vulkan"))]
fn count_devices(_compiled: Backend) -> Option<usize> {
    None
}

impl Default for GpuSettings {
    fn default() -> Self {
        Self::cpu()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Probe stub that fails the test if it runs.
    fn no_probe() -> Option<usize> {
        panic!("the device probe must not run here");
    }

    #[test]
    fn test_resolve_cpu_build_ignores_flags() {
        for no_gpu in [false, true] {
            let s = GpuSettings::resolve(Backend::Cpu, no_gpu, 3, no_probe);
            assert!(!s.uses_gpu());
            assert_eq!(s.reason, Reason::CpuBuild);
        }
    }

    #[test]
    fn test_resolve_gpu_build_uses_device() {
        let s = GpuSettings::resolve(Backend::Cuda, false, 1, || None);
        assert_eq!(s.backend, Backend::Cuda);
        assert_eq!(s.device, 1);
        assert_eq!(s.reason, Reason::Gpu);
        assert!(s.uses_gpu());
    }

    #[test]
    fn test_resolve_no_gpu_forces_cpu_without_probing() {
        let s = GpuSettings::resolve(Backend::Vulkan, true, 1, no_probe);
        assert_eq!(s, GpuSettings::cpu());
        assert_eq!(s.reason, Reason::Disabled);
    }

    #[test]
    fn test_resolve_no_device_falls_back_to_cpu() {
        let s = GpuSettings::resolve(Backend::Vulkan, false, 0, || Some(0));
        assert!(!s.uses_gpu());
        assert_eq!(s.reason, Reason::NoDevice);
    }

    #[test]
    fn test_resolve_device_in_range_uses_gpu() {
        let s = GpuSettings::resolve(Backend::Vulkan, false, 1, || Some(2));
        assert_eq!(s.backend, Backend::Vulkan);
        assert_eq!(s.device, 1);
    }

    #[test]
    fn test_resolve_device_out_of_range_falls_back_to_cpu() {
        for device in [2, 7, -1] {
            let s = GpuSettings::resolve(Backend::Vulkan, false, device, || Some(2));
            assert!(!s.uses_gpu());
            assert_eq!(
                s.reason,
                Reason::NoSuchDevice {
                    requested: device,
                    available: 2
                }
            );
        }
    }

    #[test]
    fn test_context_params_gpu() {
        let p = GpuSettings::resolve(Backend::Metal, false, 2, || None).context_params();
        assert!(p.use_gpu);
        assert!(p.flash_attn);
        assert_eq!(p.gpu_device, 2);
    }

    #[test]
    fn test_context_params_cpu_matches_cpu_build() {
        // A GPU build without a usable GPU must decode exactly like the CPU
        // build, so flash attention stays off.
        for s in [
            GpuSettings::cpu(),
            GpuSettings::resolve(Backend::Vulkan, false, 0, || Some(0)),
        ] {
            let p = s.context_params();
            assert!(!p.use_gpu);
            assert!(!p.flash_attn);
            assert_eq!(p.gpu_device, 0);
        }
    }

    #[test]
    fn test_backend_names_and_kind() {
        assert_eq!(Backend::Cpu.name(), "CPU");
        assert!(!Backend::Cpu.is_gpu());
        for b in [Backend::Cuda, Backend::Vulkan, Backend::Metal] {
            assert!(b.is_gpu());
        }
    }

    #[test]
    fn test_describe() {
        let gpu = GpuSettings::resolve(Backend::Cuda, false, 0, || None);
        assert_eq!(gpu.describe(), "CUDA (device 0)");
        let cpu_reasons = [
            Reason::CpuBuild,
            Reason::Disabled,
            Reason::NoDevice,
            Reason::NoSuchDevice {
                requested: 3,
                available: 1,
            },
        ];
        for reason in cpu_reasons {
            assert!(GpuSettings::on_cpu(reason).describe().starts_with("CPU ("));
        }
        let missing = GpuSettings::on_cpu(Reason::NoSuchDevice {
            requested: 3,
            available: 1,
        });
        assert!(missing.describe().contains("GPU device 3 not found; 1 "));
    }

    #[test]
    fn test_compiled_matches_features() {
        let gpu_feature = cfg!(any(feature = "cuda", feature = "vulkan", feature = "metal"));
        assert_eq!(Backend::compiled().is_gpu(), gpu_feature);
    }

    #[test]
    fn test_count_devices_skips_non_vulkan_backends() {
        assert_eq!(count_devices(Backend::Cpu), None);
        assert_eq!(count_devices(Backend::Metal), None);
    }
}
