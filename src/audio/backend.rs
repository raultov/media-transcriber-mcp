//! Compute backend selection for whisper.cpp.
//!
//! The backend is fixed at build time by the `cuda`, `vulkan` and `metal`
//! Cargo features; at run time the user can only turn the GPU off or pick a
//! device. Everything here is plain data so it can be tested without a GPU.

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

/// Run-time GPU choice, resolved once from the CLI flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuSettings {
    /// Backend to request from whisper.cpp for this run.
    pub backend: Backend,
    /// GPU device index, for machines with more than one GPU.
    pub device: i32,
}

impl GpuSettings {
    /// CPU decoding regardless of how the binary was built.
    pub const fn cpu() -> Self {
        Self {
            backend: Backend::Cpu,
            device: 0,
        }
    }

    /// Resolves the CLI flags against the compiled backend.
    pub const fn from_flags(no_gpu: bool, device: i32) -> Self {
        Self::resolve(Backend::compiled(), no_gpu, device)
    }

    /// [`GpuSettings::from_flags`] with the compiled backend injected, so
    /// every combination is testable from a CPU-only build.
    const fn resolve(compiled: Backend, no_gpu: bool, device: i32) -> Self {
        if no_gpu || !compiled.is_gpu() {
            Self::cpu()
        } else {
            Self {
                backend: compiled,
                device,
            }
        }
    }

    pub const fn uses_gpu(self) -> bool {
        self.backend.is_gpu()
    }

    /// One-line description for the startup log.
    pub fn describe(self) -> String {
        if self.uses_gpu() {
            format!("{} (device {})", self.backend.name(), self.device)
        } else if Backend::compiled().is_gpu() {
            format!(
                "CPU (GPU disabled; built with {})",
                Backend::compiled().name()
            )
        } else {
            "CPU (built without GPU support)".to_string()
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

impl Default for GpuSettings {
    fn default() -> Self {
        Self::cpu()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_cpu_build_ignores_flags() {
        assert_eq!(
            GpuSettings::resolve(Backend::Cpu, false, 3),
            GpuSettings::cpu()
        );
        assert_eq!(
            GpuSettings::resolve(Backend::Cpu, true, 0),
            GpuSettings::cpu()
        );
    }

    #[test]
    fn test_resolve_gpu_build_uses_device() {
        let s = GpuSettings::resolve(Backend::Cuda, false, 1);
        assert_eq!(s.backend, Backend::Cuda);
        assert_eq!(s.device, 1);
        assert!(s.uses_gpu());
    }

    #[test]
    fn test_resolve_no_gpu_forces_cpu() {
        assert_eq!(
            GpuSettings::resolve(Backend::Vulkan, true, 1),
            GpuSettings::cpu()
        );
    }

    #[test]
    fn test_context_params_gpu() {
        let p = GpuSettings::resolve(Backend::Metal, false, 2).context_params();
        assert!(p.use_gpu);
        assert!(p.flash_attn);
        assert_eq!(p.gpu_device, 2);
    }

    #[test]
    fn test_context_params_cpu() {
        let p = GpuSettings::cpu().context_params();
        assert!(!p.use_gpu);
        assert!(!p.flash_attn);
        assert_eq!(p.gpu_device, 0);
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
        assert!(
            GpuSettings::resolve(Backend::Cuda, false, 0)
                .describe()
                .starts_with("CUDA (device 0)")
        );
        assert!(GpuSettings::cpu().describe().starts_with("CPU"));
    }

    #[test]
    fn test_compiled_matches_features() {
        let gpu_feature = cfg!(any(feature = "cuda", feature = "vulkan", feature = "metal"));
        assert_eq!(Backend::compiled().is_gpu(), gpu_feature);
    }
}
