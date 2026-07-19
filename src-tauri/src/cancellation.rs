use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::sync::Notify;

/// Generation-based cancellation that cannot be accidentally reset while an
/// older operation is still unwinding.
#[derive(Default)]
pub struct CancellationSignal {
    generation: AtomicU64,
    notify: Notify,
}

impl CancellationSignal {
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self, generation: u64) -> bool {
        self.generation() != generation
    }

    pub async fn run<F>(&self, generation: u64, future: F) -> Option<F::Output>
    where
        F: Future,
    {
        // Creating Notified before checking the generation closes the race in
        // which cancellation happens between the check and waiter creation.
        let notified = self.notify.notified();
        tokio::pin!(notified);
        if self.is_cancelled(generation) {
            return None;
        }
        tokio::select! {
            value = future => (!self.is_cancelled(generation)).then_some(value),
            _ = &mut notified => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CancellationSignal;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn observes_cancellation_that_precedes_waiting() {
        let signal = CancellationSignal::default();
        let generation = signal.generation();
        signal.cancel();

        assert_eq!(signal.run(generation, std::future::ready(42)).await, None);
    }

    #[tokio::test]
    async fn cancels_an_active_wait_without_poisoning_new_work() {
        let signal = Arc::new(CancellationSignal::default());
        let generation = signal.generation();
        let waiting = {
            let signal = signal.clone();
            tokio::spawn(async move {
                signal
                    .run(generation, tokio::time::sleep(Duration::from_secs(30)))
                    .await
            })
        };
        tokio::task::yield_now().await;
        signal.cancel();

        assert_eq!(waiting.await.expect("wait task"), None);
        assert_eq!(
            signal.run(signal.generation(), std::future::ready(7)).await,
            Some(7)
        );
    }
}
