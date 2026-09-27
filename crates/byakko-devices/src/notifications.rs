//! A bounded, wake-driven mailbox for native input notifications.
use byakko_core::contract::DeviceChange;
use std::{
    collections::VecDeque,
    future::poll_fn,
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Poll, Waker},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Event {
    pub generation: u64,
    pub result: Result<DeviceChange, String>,
}

#[derive(Default)]
struct State {
    pending: VecDeque<Event>,
    waker: Option<Waker>,
    closed: bool,
}

#[derive(Clone)]
pub struct Notifications(Arc<Mutex<State>>);
impl PartialEq for Notifications {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Notifications {}
impl Hash for Notifications {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}
impl Notifications {
    pub async fn next(&self) -> Option<Event> {
        poll_fn(|cx| {
            let mut state = self.0.lock().expect("notification mailbox");
            if let Some(event) = state.pending.pop_front() {
                Poll::Ready(Some(event))
            } else if state.closed {
                Poll::Ready(None)
            } else {
                state.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
    fn push(&self, event: Event) {
        let wake = {
            let mut state = self.0.lock().expect("notification mailbox");
            if state.closed {
                return;
            }
            state
                .pending
                .retain(|queued| queued.generation == event.generation);
            if !state.pending.iter().any(|queued| {
                queued.generation == event.generation && queued.result == event.result
            }) {
                // At most the three change categories and one terminal error.
                state.pending.push_back(event);
            }
            state.waker.take()
        };
        if let Some(waker) = wake {
            waker.wake();
        }
    }
    fn close(&self) {
        let wake = {
            let mut state = self.0.lock().expect("notification mailbox");
            state.closed = true;
            state.waker.take()
        };
        if let Some(waker) = wake {
            waker.wake();
        }
    }
}

pub(crate) struct Observer {
    pub notifications: Notifications,
    stop: Arc<AtomicBool>,
}
impl Observer {
    pub fn spawn(
        generation: Arc<AtomicU64>,
        mut read: impl FnMut(Duration) -> Result<Option<DeviceChange>, String> + Send + 'static,
    ) -> std::io::Result<Self> {
        let notifications = Notifications(Arc::new(Mutex::new(State::default())));
        let outgoing = notifications.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("keyboard-notifications".into())
            .spawn(move || {
                let timeout = Duration::from_millis(100);
                while !cancelled.load(Ordering::Acquire) {
                    let current = generation.load(Ordering::Acquire);
                    if current == 0 {
                        std::thread::sleep(timeout);
                        continue;
                    }
                    let result = match read(timeout) {
                        Ok(None) => continue,
                        Ok(Some(change)) => Ok(change),
                        Err(reason) => Err(reason),
                    };
                    let failed = result.is_err();
                    if !cancelled.load(Ordering::Acquire)
                        && current == generation.load(Ordering::Acquire)
                    {
                        outgoing.push(Event {
                            generation: current,
                            result,
                        });
                    }
                    if failed {
                        break;
                    }
                }
                outgoing.close();
            })?;
        Ok(Self {
            notifications,
            stop,
        })
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.notifications.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        sync::atomic::AtomicUsize,
        task::{Context, Wake},
    };

    #[derive(Default)]
    struct WakeCount(AtomicUsize);
    impl Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn notifications_wake_waiters_coalesce_and_end_without_polling() {
        let source = Notifications(Arc::new(Mutex::new(State::default())));
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(Arc::clone(&wakes));
        let mut context = Context::from_waker(&waker);
        let mut next = Box::pin(source.next());
        assert!(next.as_mut().poll(&mut context).is_pending());
        for _ in 0..100 {
            source.push(Event {
                generation: 1,
                result: Ok(DeviceChange::Lighting),
            });
        }
        assert_eq!(source.0.lock().unwrap().pending.len(), 1);
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert!(matches!(
            next.as_mut().poll(&mut context),
            Poll::Ready(Some(Event {
                result: Ok(DeviceChange::Lighting),
                ..
            }))
        ));
        drop(next);
        source.push(Event {
            generation: 1,
            result: Ok(DeviceChange::Settings),
        });
        source.push(Event {
            generation: 2,
            result: Ok(DeviceChange::Configuration),
        });
        assert_eq!(source.0.lock().unwrap().pending.len(), 1);
        assert!(matches!(
            Box::pin(source.next()).as_mut().poll(&mut context),
            Poll::Ready(Some(Event { generation: 2, .. }))
        ));
        let mut next = Box::pin(source.next());
        assert!(next.as_mut().poll(&mut context).is_pending());
        source.close();
        assert!(matches!(
            next.as_mut().poll(&mut context),
            Poll::Ready(None)
        ));
    }
}
