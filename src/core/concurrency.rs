use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use atomic_waker::AtomicWaker;

#[derive(Clone)]
pub(crate) struct WorkerConcurrencyLimit {
    inner: Option<Arc<ConcurrencyState>>,
}

struct ConcurrencyState {
    max: usize,
    active: AtomicUsize,
    waiter: AtomicWaker,
}

pub(crate) struct ConcurrencyPermit {
    state: Option<Arc<ConcurrencyState>>,
}

pub(crate) struct AcquirePermit {
    state: Option<Arc<ConcurrencyState>>,
}

impl WorkerConcurrencyLimit {
    pub(crate) fn new(max: Option<usize>) -> Self {
        match max {
            Some(max) if max > 0 => Self {
                inner: Some(Arc::new(ConcurrencyState {
                    max,
                    active: AtomicUsize::new(0),
                    waiter: AtomicWaker::new(),
                })),
            },
            _ => Self { inner: None },
        }
    }

    pub(crate) fn acquire(&self) -> AcquirePermit {
        AcquirePermit {
            state: self.inner.clone(),
        }
    }

    pub(crate) fn try_acquire(&self) -> Option<ConcurrencyPermit> {
        let Some(state) = self.inner.as_ref() else {
            return Some(ConcurrencyPermit { state: None });
        };

        loop {
            let active = state.active.load(Ordering::Acquire);
            if active >= state.max {
                return None;
            }
            if state
                .active
                .compare_exchange_weak(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(ConcurrencyPermit {
                    state: Some(Arc::clone(state)),
                });
            }
        }
    }
}

impl Future for AcquirePermit {
    type Output = ConcurrencyPermit;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(state) = self.state.as_ref() else {
            return Poll::Ready(ConcurrencyPermit { state: None });
        };

        loop {
            let active = state.active.load(Ordering::Acquire);
            if active < state.max {
                if state
                    .active
                    .compare_exchange_weak(
                        active,
                        active + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    return Poll::Ready(ConcurrencyPermit {
                        state: Some(Arc::clone(state)),
                    });
                }
                continue;
            }

            state.waiter.register(cx.waker());
            if state.active.load(Ordering::Acquire) < state.max {
                continue;
            }
            return Poll::Pending;
        }
    }
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        let mut active = state.active.load(Ordering::Acquire);
        while active > 0 {
            match state.active.compare_exchange_weak(
                active,
                active - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    state.waiter.wake();
                    return;
                }
                Err(current) => active = current,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerConcurrencyLimit;

    #[test]
    fn try_acquire_is_unlimited_without_configured_max() {
        let limit = WorkerConcurrencyLimit::new(None);

        let _first = limit.try_acquire().unwrap();
        let _second = limit.try_acquire().unwrap();
    }

    #[test]
    fn try_acquire_returns_none_while_limit_is_full() {
        let limit = WorkerConcurrencyLimit::new(Some(1));

        let first = limit.try_acquire().unwrap();
        assert!(limit.try_acquire().is_none());

        drop(first);
        assert!(limit.try_acquire().is_some());
    }
}
