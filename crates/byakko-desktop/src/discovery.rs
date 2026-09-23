//! Read-only host discovery. The probe is supplied by the composition root.
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    Missing,
    Ready { id: String },
    Ambiguous { count: usize },
    Error(String),
}

pub struct Discovery {
    requests: SyncSender<u64>,
    results: Receiver<(u64, Availability)>,
    next_scan: u64,
    pending: Option<Pending>,
}

#[derive(Clone, Copy)]
enum Pending {
    Current(u64),
    Discard(u64),
}

impl Discovery {
    pub fn spawn(probe: impl Fn() -> Availability + Send + 'static) -> std::io::Result<Self> {
        let (requests, requested) = mpsc::sync_channel::<u64>(1);
        let (completed, results) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("byakko-discovery".into())
            .spawn(move || {
                for scan in requested {
                    if completed.send((scan, probe())).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            results,
            next_scan: 0,
            pending: None,
        })
    }

    pub fn request(&mut self) {
        if self.pending.is_some() {
            return;
        }
        let Some(scan) = self.next_scan.checked_add(1) else {
            return;
        };
        if self.requests.try_send(scan).is_ok() {
            self.next_scan = scan;
            self.pending = Some(Pending::Current(scan));
        }
    }

    /// A device command supersedes any earlier enumeration result. Its reply
    /// will still be drained, but can no longer change the connection state.
    pub fn invalidate(&mut self) {
        if let Some(Pending::Current(scan)) = self.pending {
            self.pending = Some(Pending::Discard(scan));
        }
    }

    pub fn receive(&mut self) -> Option<Availability> {
        match self.results.try_recv() {
            Ok((scan, availability)) if matches!(self.pending, Some(Pending::Current(id)) if id == scan) =>
            {
                self.pending = None;
                Some(availability)
            }
            Ok((scan, _)) if matches!(self.pending, Some(Pending::Discard(id)) if id == scan) => {
                self.pending = None;
                None
            }
            Ok(_) => None,
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                Some(Availability::Error("Discovery worker stopped".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_invalidates_older_scan_reply() {
        let (answers, replies) = mpsc::sync_channel(0);
        let mut discovery = Discovery::spawn(move || replies.recv().unwrap()).unwrap();
        discovery.request();
        discovery.invalidate();
        answers.send(Availability::Missing).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while discovery.pending.is_some() && std::time::Instant::now() < deadline {
            assert_eq!(discovery.receive(), None);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(discovery.pending.is_none());
        discovery.request();
        answers
            .send(Availability::Ready {
                id: "second".into(),
            })
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            if let Some(result) = discovery.receive() {
                assert_eq!(
                    result,
                    Availability::Ready {
                        id: "second".into()
                    }
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("replacement scan never completed");
    }
}
