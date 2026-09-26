//! Bounded background enumeration, independent of the selected-device worker.
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    Unknown,
    Missing,
    Ready { id: String },
    Ambiguous { count: usize },
    Error(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pending {
    Current(u64),
    Discard(u64),
}

enum State {
    Idle,
    Pending(Pending),
    Failed(String),
    Stopped,
}

pub struct Discovery {
    requests: SyncSender<u64>,
    results: Receiver<(u64, Availability)>,
    next_id: u64,
    state: State,
}

impl Discovery {
    pub fn spawn(probe: impl Fn() -> Availability + Send + 'static) -> io::Result<Self> {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, results) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("device-discovery".into())
            .spawn(move || {
                while let Ok(id) = incoming.recv() {
                    if outgoing.send((id, probe())).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            results,
            next_id: 0,
            state: State::Idle,
        })
    }

    pub fn request(&mut self) {
        if !matches!(self.state, State::Idle) {
            return;
        }
        let Some(id) = self.next_id.checked_add(1) else {
            self.state = State::Failed("Discovery scan IDs exhausted".into());
            return;
        };
        self.next_id = id;
        self.state = State::Pending(Pending::Current(id));
        // A failed send is delivered once through the disconnected result channel.
        let _ = self.requests.try_send(self.next_id);
    }

    pub fn invalidate(&mut self) {
        if let State::Pending(Pending::Current(id)) = self.state {
            self.state = State::Pending(Pending::Discard(id));
        }
    }

    pub fn pending(&self) -> bool {
        matches!(self.state, State::Pending(_))
    }

    pub fn receive(&mut self) -> Option<Availability> {
        match &self.state {
            State::Failed(_) => {
                let State::Failed(error) = std::mem::replace(&mut self.state, State::Stopped)
                else {
                    unreachable!()
                };
                return Some(Availability::Error(error));
            }
            State::Stopped => return None,
            State::Idle | State::Pending(_) => {}
        }
        match self.results.try_recv() {
            Ok((id, result)) => {
                let active = std::mem::replace(&mut self.state, State::Idle);
                if matches!(active, State::Pending(Pending::Current(current)) if current == id) {
                    Some(result)
                } else {
                    None
                }
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.state = State::Stopped;
                Some(Availability::Error("Discovery worker disconnected".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn wait_result(discovery: &mut Discovery) -> Availability {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(result) = discovery.receive() {
                return result;
            }
            assert!(Instant::now() < deadline, "Discovery result timed out");
            thread::yield_now();
        }
    }

    #[test]
    fn invalidated_scan_drains_before_another_request() {
        let (started, starts) = mpsc::channel();
        let (release, releases) = mpsc::channel();
        let releases = Arc::new(Mutex::new(releases));
        let mut discovery = Discovery::spawn(move || {
            started.send(()).unwrap();
            releases.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
            Availability::Missing
        })
        .unwrap();
        discovery.request();
        starts.recv_timeout(TIMEOUT).unwrap();
        discovery.invalidate();
        discovery.request();
        assert!(discovery.pending());
        assert!(starts.try_recv().is_err());
        release.send(()).unwrap();
        let deadline = Instant::now() + TIMEOUT;
        while discovery.pending() {
            assert_eq!(discovery.receive(), None);
            assert!(Instant::now() < deadline, "Invalidated scan did not drain");
            thread::yield_now();
        }
        discovery.request();
        starts.recv_timeout(TIMEOUT).unwrap();
        release.send(()).unwrap();
        assert_eq!(wait_result(&mut discovery), Availability::Missing);
    }

    #[test]
    fn panic_is_reported_only_once() {
        let mut discovery = Discovery::spawn(|| panic!("probe failure")).unwrap();
        discovery.request();
        assert!(matches!(
            wait_result(&mut discovery),
            Availability::Error(_)
        ));
        assert_eq!(discovery.receive(), None);
        discovery.request();
        assert!(!discovery.pending());
    }

    #[test]
    fn scan_id_exhaustion_is_terminal_and_reported_once() {
        let mut discovery = Discovery::spawn(|| Availability::Missing).unwrap();
        discovery.next_id = u64::MAX;
        discovery.request();
        assert!(!discovery.pending());
        assert_eq!(
            discovery.receive(),
            Some(Availability::Error("Discovery scan IDs exhausted".into()))
        );
        discovery.request();
        assert_eq!(discovery.receive(), None);
    }
}
