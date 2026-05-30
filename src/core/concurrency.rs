use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Clone)]
pub(crate) struct WorkerConcurrencyLimit {
    inner: Option<Arc<ConcurrencyState>>,
}

struct ConcurrencyState {
    max: usize,
    state: Mutex<ConcurrencyStateInner>,
}

struct ConcurrencyStateInner {
    active: usize,
    waiters: Vec<Waker>,
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
                    state: Mutex::new(ConcurrencyStateInner {
                        active: 0,
                        waiters: Vec::new(),
                    }),
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
}

impl Future for AcquirePermit {
    type Output = ConcurrencyPermit;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(state) = self.state.as_ref() else {
            return Poll::Ready(ConcurrencyPermit { state: None });
        };

        let Ok(mut inner) = state.state.lock() else {
            return Poll::Pending;
        };
        if inner.active < state.max {
            inner.active += 1;
            return Poll::Ready(ConcurrencyPermit {
                state: Some(Arc::clone(state)),
            });
        }
        if !inner
            .waiters
            .iter()
            .any(|waker| waker.will_wake(cx.waker()))
        {
            inner.waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        let Ok(mut inner) = state.state.lock() else {
            return;
        };
        if inner.active > 0 {
            inner.active -= 1;
        }
        if let Some(waker) = inner.waiters.pop() {
            waker.wake();
        }
    }
}
