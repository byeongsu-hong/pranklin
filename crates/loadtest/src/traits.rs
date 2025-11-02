use std::time::Duration;
use tokio::task::JoinHandle;

/// Extension trait for spawning workers
pub trait WorkerSpawner: Sized + Clone + Send + 'static {
    fn spawn_workers<F>(
        self,
        num_workers: usize,
        worker_fn: F,
    ) -> Vec<JoinHandle<()>>
    where
        F: Fn(Self) + Send + Clone + 'static,
    {
        (0..num_workers)
            .map(|_| {
                let item = self.clone();
                let func = worker_fn.clone();
                tokio::spawn(async move { func(item) })
            })
            .collect()
    }
}

impl<T: Clone + Send + 'static> WorkerSpawner for T {}

/// Extension trait for Duration
pub trait DurationExt {
    fn has_elapsed_since(&self, start: std::time::Instant) -> bool;
}

impl DurationExt for Duration {
    fn has_elapsed_since(&self, start: std::time::Instant) -> bool {
        start.elapsed() >= *self
    }
}

/// Extension trait for random range generation
pub trait RandomRange {
    type Output;
    fn random_in_range(min: Self::Output, max: Self::Output) -> Self::Output;
}

impl RandomRange for u64 {
    type Output = u64;
    fn random_in_range(min: u64, max: u64) -> u64 {
        fastrand::u64(min..max)
    }
}

impl RandomRange for u128 {
    type Output = u128;
    fn random_in_range(min: u128, max: u128) -> u128 {
        fastrand::u128(min..max)
    }
}

