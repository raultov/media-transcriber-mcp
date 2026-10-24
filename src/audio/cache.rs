//! Keeps the loaded Whisper model in memory between tool calls.
//!
//! Loading `large-v3-turbo-q5_0` reads ~550 MB and, on a GPU build, uploads
//! it to VRAM. Doing that on every call dominates short clips and agents that
//! transcribe long media in windows. The cache holds at most one model: a
//! different model (or GPU setting) replaces it, and an idle timeout frees it
//! so a long-running server does not pin memory forever.
//!
//! The cache is generic over the model type so its policy is unit tested
//! without loading a real model.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::backend::GpuSettings;

/// Idle time after which a loaded model is freed, unless configured.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// Longest wait between two idle checks, so a long timeout still frees the
/// model close to when it expires.
const MAX_IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Converts the `--model-idle-timeout` seconds; `0` disables eviction.
pub fn idle_timeout_from_secs(secs: u64) -> Option<Duration> {
    (secs > 0).then(|| Duration::from_secs(secs))
}

/// What a loaded model depends on: the file and how it was loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelKey {
    pub path: PathBuf,
    pub gpu: GpuSettings,
}

impl ModelKey {
    /// File name for logs, falling back to the full path.
    pub fn file_name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

/// How [`ModelCache::get_or_load`] obtained the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The model was already loaded.
    Hit,
    /// The model was loaded now; `replaced` is the model it evicted, if any.
    Miss { replaced: Option<ModelKey> },
}

#[derive(Debug)]
struct Entry<T> {
    key: ModelKey,
    model: Arc<T>,
    last_used: Instant,
}

/// At most one loaded model, reused while the [`ModelKey`] does not change.
#[derive(Debug)]
pub struct ModelCache<T> {
    entry: Option<Entry<T>>,
    idle_timeout: Option<Duration>,
}

/// The cache shared by the tool handler and the idle evictor.
pub type SharedModelCache<T> = Arc<Mutex<ModelCache<T>>>;

impl<T> ModelCache<T> {
    /// `idle_timeout` of `None` keeps the model until the process exits.
    pub fn new(idle_timeout: Option<Duration>) -> Self {
        Self {
            entry: None,
            idle_timeout,
        }
    }

    pub fn idle_timeout(&self) -> Option<Duration> {
        self.idle_timeout
    }

    /// Key of the loaded model, if any.
    pub fn loaded(&self) -> Option<&ModelKey> {
        self.entry.as_ref().map(|e| &e.key)
    }

    /// Returns the model for `key`, calling `load` only when it is not the
    /// one already loaded.
    ///
    /// A different model is dropped *before* `load` runs, so two models never
    /// occupy RAM or VRAM at the same time. If `load` fails the cache is left
    /// empty and the error is returned.
    pub fn get_or_load<E>(
        &mut self,
        key: &ModelKey,
        now: Instant,
        load: impl FnOnce() -> Result<T, E>,
    ) -> Result<(Arc<T>, Lookup), E> {
        if let Some(entry) = self.entry.as_mut().filter(|e| &e.key == key) {
            entry.last_used = now;
            return Ok((Arc::clone(&entry.model), Lookup::Hit));
        }

        let replaced = self.entry.take().map(|e| e.key);
        let model = Arc::new(load()?);
        self.entry = Some(Entry {
            key: key.clone(),
            model: Arc::clone(&model),
            last_used: now,
        });
        Ok((model, Lookup::Miss { replaced }))
    }

    /// Hands back a model obtained from [`ModelCache::get_or_load`] once the
    /// transcription is done, so the idle time starts now rather than when
    /// the transcription started.
    pub fn release(&mut self, model: Arc<T>, now: Instant) {
        if let Some(entry) = self
            .entry
            .as_mut()
            .filter(|e| Arc::ptr_eq(&e.model, &model))
        {
            entry.last_used = now;
        }
    }

    /// Frees the model if it has been idle for the timeout, returning its key.
    ///
    /// A model still held by a running transcription is never freed; it
    /// counts as used at `now` instead. That only matters if the holder never
    /// calls [`ModelCache::release`] (for example after a panic).
    pub fn evict_idle(&mut self, now: Instant) -> Option<ModelKey> {
        let timeout = self.idle_timeout?;
        let entry = self.entry.as_mut()?;
        if Arc::strong_count(&entry.model) > 1 {
            entry.last_used = now;
            return None;
        }
        if now.saturating_duration_since(entry.last_used) < timeout {
            return None;
        }
        self.entry.take().map(|e| e.key)
    }

    /// Interval between idle checks, or `None` when eviction is disabled.
    fn check_interval(&self) -> Option<Duration> {
        self.idle_timeout.map(|t| t.min(MAX_IDLE_CHECK_INTERVAL))
    }
}

impl<T> Default for ModelCache<T> {
    fn default() -> Self {
        Self::new(Some(DEFAULT_IDLE_TIMEOUT))
    }
}

/// Starts the background task that frees an idle model.
///
/// Returns `None` when eviction is disabled. The check uses `try_lock`, so a
/// tick that lands while a model is being loaded is skipped instead of
/// blocking a runtime thread for the duration of the load.
pub fn spawn_idle_evictor<T>(cache: SharedModelCache<T>) -> Option<tokio::task::JoinHandle<()>>
where
    T: Send + Sync + 'static,
{
    let interval = lock(&cache).check_interval()?;
    Some(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let Ok(mut guard) = cache.try_lock() else {
                continue;
            };
            if let Some(key) = guard.evict_idle(Instant::now()) {
                eprintln!(
                    "Whisper model: unloaded {} after {} s idle",
                    key.file_name(),
                    guard.idle_timeout().unwrap_or_default().as_secs()
                );
            }
        }
    }))
}

/// Locks the cache, recovering from a panic in a previous holder: the cache
/// only holds plain data, so its state is consistent at any point.
pub fn lock<T>(cache: &Mutex<ModelCache<T>>) -> std::sync::MutexGuard<'_, ModelCache<T>> {
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn key(name: &str) -> ModelKey {
        ModelKey {
            path: PathBuf::from(format!("/models/{name}.bin")),
            gpu: GpuSettings::cpu(),
        }
    }

    /// Loader that counts its calls and returns a numbered fake model.
    fn counting_loader(calls: &Cell<u32>) -> impl FnOnce() -> Result<u32, String> + '_ {
        move || {
            calls.set(calls.get() + 1);
            Ok(calls.get())
        }
    }

    #[test]
    fn test_first_lookup_is_a_miss_then_hits() {
        let mut cache = ModelCache::new(None);
        let calls = Cell::new(0);
        let t0 = Instant::now();

        let (model, lookup) = cache
            .get_or_load(&key("a"), t0, counting_loader(&calls))
            .unwrap();
        assert_eq!(lookup, Lookup::Miss { replaced: None });
        assert_eq!(*model, 1);

        let (again, lookup) = cache
            .get_or_load(&key("a"), t0, counting_loader(&calls))
            .unwrap();
        assert_eq!(lookup, Lookup::Hit);
        assert!(Arc::ptr_eq(&model, &again));
        assert_eq!(calls.get(), 1, "a hit must not load again");
    }

    #[test]
    fn test_other_model_replaces_and_frees_the_previous_one() {
        let mut cache = ModelCache::new(None);
        let calls = Cell::new(0);
        let t0 = Instant::now();
        let (first, _) = cache
            .get_or_load(&key("a"), t0, counting_loader(&calls))
            .unwrap();
        let weak = Arc::downgrade(&first);
        drop(first);

        let (_, lookup) = cache
            .get_or_load(&key("b"), t0, counting_loader(&calls))
            .unwrap();
        assert_eq!(
            lookup,
            Lookup::Miss {
                replaced: Some(key("a"))
            }
        );
        assert!(weak.upgrade().is_none(), "the previous model must be freed");
        assert_eq!(cache.loaded(), Some(&key("b")));
    }

    #[test]
    fn test_previous_model_is_dropped_before_loading_the_next() {
        let mut cache = ModelCache::new(None);
        let t0 = Instant::now();
        let (first, _) = cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();
        let weak = Arc::downgrade(&first);
        drop(first);

        cache
            .get_or_load(&key("b"), t0, || {
                assert!(
                    weak.upgrade().is_none(),
                    "old model still alive during load"
                );
                Ok::<_, String>(2)
            })
            .unwrap();
    }

    #[test]
    fn test_gpu_settings_are_part_of_the_key() {
        let mut cache = ModelCache::new(None);
        let calls = Cell::new(0);
        let t0 = Instant::now();
        let cpu = key("a");
        let mut other = cpu.clone();
        other.gpu.device = 1;

        cache
            .get_or_load(&cpu, t0, counting_loader(&calls))
            .unwrap();
        let (_, lookup) = cache
            .get_or_load(&other, t0, counting_loader(&calls))
            .unwrap();
        assert!(matches!(lookup, Lookup::Miss { .. }));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn test_failed_load_leaves_cache_empty() {
        let mut cache: ModelCache<u32> = ModelCache::new(None);
        let t0 = Instant::now();
        cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();

        let err = cache
            .get_or_load(&key("b"), t0, || Err("corrupt model".to_string()))
            .unwrap_err();
        assert_eq!(err, "corrupt model");
        assert_eq!(cache.loaded(), None);
    }

    #[test]
    fn test_evict_idle_after_timeout_only() {
        let timeout = Duration::from_secs(60);
        let mut cache = ModelCache::new(Some(timeout));
        let t0 = Instant::now();
        cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();

        assert_eq!(cache.evict_idle(t0 + Duration::from_secs(59)), None);
        assert_eq!(cache.evict_idle(t0 + timeout), Some(key("a")));
        assert_eq!(cache.loaded(), None);
    }

    #[test]
    fn test_hit_resets_the_idle_clock() {
        let mut cache = ModelCache::new(Some(Duration::from_secs(60)));
        let t0 = Instant::now();
        cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();
        let t1 = t0 + Duration::from_secs(50);
        cache
            .get_or_load(&key("a"), t1, || Ok::<_, String>(2))
            .unwrap();

        assert_eq!(cache.evict_idle(t0 + Duration::from_secs(100)), None);
        assert!(cache.evict_idle(t1 + Duration::from_secs(60)).is_some());
    }

    #[test]
    fn test_model_in_use_is_never_evicted() {
        let mut cache = ModelCache::new(Some(Duration::from_secs(60)));
        let t0 = Instant::now();
        let (in_use, _) = cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();

        let during = t0 + Duration::from_secs(600);
        assert_eq!(cache.evict_idle(during), None);
        drop(in_use);
        // Idle time counts from the last check that saw it in use.
        assert_eq!(cache.evict_idle(during + Duration::from_secs(59)), None);
        assert!(cache.evict_idle(during + Duration::from_secs(60)).is_some());
    }

    #[test]
    fn test_release_starts_the_idle_clock() {
        let timeout = Duration::from_secs(60);
        let mut cache = ModelCache::new(Some(timeout));
        let t0 = Instant::now();
        let (model, _) = cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();

        // A check while in use, then the transcription ends long after it.
        assert_eq!(cache.evict_idle(t0 + Duration::from_secs(30)), None);
        let done = t0 + Duration::from_secs(50);
        cache.release(model, done);

        assert_eq!(cache.evict_idle(done + Duration::from_secs(59)), None);
        assert!(cache.evict_idle(done + timeout).is_some());
    }

    #[test]
    fn test_release_of_a_replaced_model_is_ignored() {
        let mut cache = ModelCache::new(Some(Duration::from_secs(60)));
        let t0 = Instant::now();
        let (old, _) = cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();
        cache
            .get_or_load(&key("b"), t0, || Ok::<_, String>(2))
            .unwrap();

        cache.release(old, t0 + Duration::from_secs(3600));
        assert!(
            cache.evict_idle(t0 + Duration::from_secs(60)).is_some(),
            "releasing a stale model must not refresh the current one"
        );
    }

    #[test]
    fn test_no_timeout_never_evicts() {
        let mut cache = ModelCache::new(None);
        let t0 = Instant::now();
        cache
            .get_or_load(&key("a"), t0, || Ok::<_, String>(1))
            .unwrap();
        assert_eq!(cache.evict_idle(t0 + Duration::from_secs(86_400)), None);
        assert_eq!(cache.check_interval(), None);
    }

    #[test]
    fn test_idle_timeout_from_secs() {
        assert_eq!(idle_timeout_from_secs(0), None);
        assert_eq!(idle_timeout_from_secs(90), Some(Duration::from_secs(90)));
    }

    #[test]
    fn test_check_interval_is_capped() {
        let long = ModelCache::<u32>::new(Some(Duration::from_secs(3600)));
        assert_eq!(long.check_interval(), Some(MAX_IDLE_CHECK_INTERVAL));
        let short = ModelCache::<u32>::new(Some(Duration::from_secs(5)));
        assert_eq!(short.check_interval(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_key_file_name() {
        assert_eq!(key("ggml-base").file_name(), "ggml-base.bin");
    }

    #[tokio::test]
    async fn test_idle_evictor_frees_the_model() {
        let cache: SharedModelCache<u32> =
            Arc::new(Mutex::new(ModelCache::new(Some(Duration::from_millis(50)))));
        lock(&cache)
            .get_or_load(&key("a"), Instant::now(), || Ok::<_, String>(1))
            .unwrap();
        let task = spawn_idle_evictor(Arc::clone(&cache)).expect("eviction is enabled");

        let deadline = Instant::now() + Duration::from_secs(5);
        while lock(&cache).loaded().is_some() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        task.abort();
        assert_eq!(lock(&cache).loaded(), None);
    }

    #[tokio::test]
    async fn test_idle_evictor_disabled_without_timeout() {
        let cache: SharedModelCache<u32> = Arc::new(Mutex::new(ModelCache::new(None)));
        assert!(spawn_idle_evictor(cache).is_none());
    }
}
