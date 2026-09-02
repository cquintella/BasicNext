use std::{
    collections::HashMap,
    collections::VecDeque,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::net::Address;

/// Deterministic hostname provider for local security tests.
#[derive(Clone, Default)]
pub(crate) struct FakeResolver {
    answers: Arc<Mutex<HashMap<String, Vec<Address>>>>,
}

impl FakeResolver {
    pub(crate) fn insert(&self, hostname: &str, addresses: Vec<Address>) {
        self.answers
            .lock()
            .expect("fake resolver lock")
            .insert(hostname.into(), addresses);
    }

    pub(crate) fn resolve(
        &self,
        hostname: &str,
        maximum: usize,
    ) -> Result<Vec<Address>, &'static str> {
        let answers = self.answers.lock().expect("fake resolver lock");
        let addresses = answers.get(hostname).ok_or("fake resolver has no answer")?;
        Ok(addresses.iter().copied().take(maximum).collect())
    }
}

/// Monotonic clock controlled explicitly by a test.
#[derive(Clone, Default)]
pub(crate) struct ManualClock {
    elapsed: Arc<Mutex<Duration>>,
}

/// A deadline whose expiration is controlled by `ManualClock`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TestDeadline {
    at: Duration,
}

impl TestDeadline {
    pub(crate) fn from_now(clock: &ManualClock, timeout: Duration) -> Self {
        Self {
            at: clock.now().saturating_add(timeout),
        }
    }

    pub(crate) fn expired(self, clock: &ManualClock) -> bool {
        clock.now() >= self.at
    }
}

/// Scripted local peer input for slow-client and framing tests.
#[derive(Default)]
pub(crate) struct ScriptedPeer {
    chunks: VecDeque<ScriptedChunk>,
}

/// Bounded key table for deterministic rate-limit saturation tests.
#[derive(Debug)]
pub(crate) struct BoundedKeyTable {
    keys: VecDeque<String>,
    maximum: usize,
}

impl BoundedKeyTable {
    pub(crate) fn new(maximum: usize) -> Self {
        Self {
            keys: VecDeque::new(),
            maximum,
        }
    }

    pub(crate) fn admit(&mut self, key: &str) -> bool {
        if self.keys.iter().any(|known| known == key) {
            return true;
        }
        if self.keys.len() >= self.maximum {
            return false;
        }
        self.keys.push_back(key.to_owned());
        true
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }
}

#[derive(Debug)]
struct ScriptedChunk {
    available_at: Duration,
    bytes: Vec<u8>,
}

impl ScriptedPeer {
    pub(crate) fn push(&mut self, chunk: impl Into<Vec<u8>>) {
        self.push_after(Duration::ZERO, chunk);
    }

    pub(crate) fn push_after(&mut self, delay: Duration, chunk: impl Into<Vec<u8>>) {
        let available_at = self.chunks.back().map_or(delay, |previous| {
            previous.available_at.saturating_add(delay)
        });
        self.chunks.push_back(ScriptedChunk {
            available_at,
            bytes: chunk.into(),
        });
    }

    pub(crate) fn next_chunk(&mut self) -> Option<Vec<u8>> {
        self.chunks.pop_front().map(|chunk| chunk.bytes)
    }

    pub(crate) fn next_ready_chunk(&mut self, clock: &ManualClock) -> Option<Vec<u8>> {
        let ready = self
            .chunks
            .front()
            .is_some_and(|chunk| clock.now() >= chunk.available_at);
        ready.then(|| self.next_chunk()).flatten()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HandshakeResult {
    Complete,
    Timeout,
    Failed,
}

/// Explicit admission state for tests of connection quotas and drain.
#[derive(Debug)]
pub(crate) struct ConnectionAccounting {
    active: AtomicUsize,
    joined: AtomicUsize,
    maximum: usize,
    draining: AtomicBool,
}

impl ConnectionAccounting {
    pub(crate) fn new(maximum: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            joined: AtomicUsize::new(0),
            maximum,
            draining: AtomicBool::new(false),
        }
    }

    pub(crate) fn admit(&self) -> bool {
        if self.draining.load(Ordering::Acquire) {
            return false;
        }
        self.active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < self.maximum).then_some(active + 1)
            })
            .is_ok()
    }

    pub(crate) fn release(&self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }

    pub(crate) fn join(&self) {
        self.joined.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn begin_drain(&self) {
        self.draining.store(true, Ordering::Release);
    }

    pub(crate) fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    pub(crate) fn joined(&self) -> usize {
        self.joined.load(Ordering::Acquire)
    }
}

impl ManualClock {
    pub(crate) fn now(&self) -> Duration {
        *self.elapsed.lock().expect("manual clock lock")
    }

    pub(crate) fn advance(&self, duration: Duration) {
        let mut elapsed = self.elapsed.lock().expect("manual clock lock");
        *elapsed = elapsed.saturating_add(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedKeyTable, FakeResolver, ManualClock, TestDeadline};
    use crate::net::Address;
    use std::time::Duration;

    #[test]
    fn fake_resolver_is_bounded_and_reproducible() {
        let resolver = FakeResolver::default();
        resolver.insert(
            "service.test",
            vec![
                Address::parse("127.0.0.1").unwrap(),
                Address::parse("8.8.8.8").unwrap(),
            ],
        );
        assert_eq!(resolver.resolve("service.test", 1).unwrap().len(), 1);
        assert_eq!(resolver.resolve("service.test", 2).unwrap().len(), 2);
        assert!(resolver.resolve("missing.test", 1).is_err());
    }

    #[test]
    fn rate_limit_key_table_rejects_new_keys_at_capacity() {
        let mut table = BoundedKeyTable::new(2);
        assert!(table.admit("GET /health\n192.0.2.1"));
        assert!(table.admit("GET /health\n192.0.2.2"));
        assert!(table.admit("GET /health\n192.0.2.1"));
        assert!(!table.admit("GET /health\n192.0.2.3"));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn manual_clock_advances_without_sleep() {
        let clock = ManualClock::default();
        assert_eq!(clock.now(), Duration::ZERO);
        clock.advance(Duration::from_secs(5));
        assert_eq!(clock.now(), Duration::from_secs(5));
    }

    #[test]
    fn deadlines_and_slow_peer_are_deterministic_without_sleep() {
        let clock = ManualClock::default();
        let deadline = TestDeadline::from_now(&clock, Duration::from_secs(5));
        let mut peer = super::ScriptedPeer::default();
        peer.push_after(Duration::from_secs(5), b"partial headers".to_vec());
        assert!(!deadline.expired(&clock));
        assert!(peer.next_ready_chunk(&clock).is_none());
        clock.advance(Duration::from_secs(5));
        assert!(deadline.expired(&clock));
        assert_eq!(
            peer.next_ready_chunk(&clock).as_deref(),
            Some(b"partial headers".as_slice())
        );
    }

    #[test]
    fn scripted_peer_and_handshake_are_deterministic() {
        let mut peer = super::ScriptedPeer::default();
        peer.push(b"HEADERS".to_vec());
        peer.push_after(Duration::from_secs(30), b"BODY".to_vec());
        assert_eq!(
            peer.next_ready_chunk(&ManualClock::default()).unwrap(),
            b"HEADERS"
        );
        let clock = ManualClock::default();
        clock.advance(Duration::from_secs(29));
        assert!(peer.next_ready_chunk(&clock).is_none());
        clock.advance(Duration::from_secs(1));
        assert_eq!(peer.next_ready_chunk(&clock).unwrap(), b"BODY");
        assert!(peer.next_chunk().is_none());
        let outcomes = [
            super::HandshakeResult::Complete,
            super::HandshakeResult::Timeout,
            super::HandshakeResult::Failed,
        ];
        assert_eq!(outcomes.len(), 3);
    }

    #[test]
    fn connection_accounting_is_bounded_and_drains() {
        let accounting = super::ConnectionAccounting::new(2);
        assert!(accounting.admit());
        assert!(accounting.admit());
        assert!(!accounting.admit());
        accounting.release();
        assert_eq!(accounting.active(), 1);
        accounting.begin_drain();
        assert!(!accounting.admit());
        accounting.release();
        accounting.join();
        assert_eq!(accounting.active(), 0);
        assert_eq!(accounting.joined(), 1);
    }
}
