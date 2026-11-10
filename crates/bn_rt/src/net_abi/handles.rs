//! The table of native network handles (sockets, listeners, streams, UDP
//! packets) of the module copy that runs on this thread. Ids carry a
//! generation (`crate::handle_table_abi`): an id kept after `Close` never reaches
//! a socket opened later in the same slot and answers `Net.CLOSED`, as in
//! `bni`.

use crate::handle_table_abi::GenerationTable;
use crate::net::{TcpListener, TcpStream, UdpPacket, UdpSocket};
use std::sync::{Arc, Mutex};

pub(super) enum Handle {
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
    UdpPacket(UdpPacket),
}

/// The sockets of the module copy that runs on this thread (`bn_core_copy`):
/// a task's close when the task ends.
#[derive(Default)]
struct Sockets(Mutex<GenerationTable<Handle>>);

fn table() -> Arc<Sockets> {
    bn_core_copy::table::<Sockets>()
}

pub(super) fn insert(handle: Handle) -> Result<i64, &'static str> {
    let sockets = table();
    let mut table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    // The quota the interpreter applies too (`bn_limits`).
    table
        .insert(handle, bn_limits::web_limits().socket_handles_max)
        .ok_or("socket handle quota exceeded")
}

/// Closes `id`; a stale or unknown id is `None` (close is idempotent).
pub(super) fn remove(id: i64) -> Result<Option<Handle>, &'static str> {
    let sockets = table();
    let mut table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.remove(id))
}

pub(super) fn with<T>(
    id: i64,
    operation: impl FnOnce(&Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let sockets = table();
    let table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get(id).map(operation))
}

pub(crate) fn with_mut<T>(
    id: i64,
    operation: impl FnOnce(&mut Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let sockets = table();
    let mut table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get_mut(id).map(operation))
}

#[cfg(test)]
mod tests {
    use super::{Handle, insert, remove, with};
    use crate::net::{Address, Endpoint, UdpSocket};

    fn loopback_socket() -> UdpSocket {
        UdpSocket::bind(Endpoint::new(
            Address::parse("127.0.0.1").expect("loopback"),
            0,
        ))
        .expect("bind loopback")
    }

    #[test]
    fn a_closed_handle_never_reaches_the_socket_that_reuses_its_slot() {
        let _lock = crate::network_test_lock();
        let first = insert(Handle::UdpSocket(loopback_socket())).expect("insert handle");
        assert!(with(first, |_| ()).expect("lookup").is_some());
        assert!(remove(first).expect("close").is_some());
        let second = insert(Handle::UdpSocket(loopback_socket())).expect("insert again");
        assert_ne!(first, second, "a reused slot gets a new generation");
        assert!(with(first, |_| ()).expect("lookup stale").is_none());
        assert!(with(second, |_| ()).expect("lookup live").is_some());
        assert!(remove(first).expect("stale close").is_none());
        assert!(with(second, |_| ()).expect("still live").is_some());
        let _ = remove(second);
    }

    #[test]
    fn a_handle_cannot_be_accessed_or_closed_across_task_copies() {
        let _lock = crate::network_test_lock();
        let first = bn_core_copy::run(|| {
            insert(Handle::UdpSocket(loopback_socket())).expect("insert in first task")
        });
        bn_core_copy::run(|| {
            let second =
                insert(Handle::UdpSocket(loopback_socket())).expect("insert in second task");
            assert_ne!(
                first, second,
                "handles in different task copies must have different tags"
            );
            assert!(
                with(first, |_| ()).expect("lookup cross-task").is_none(),
                "cross-task lookup must yield None"
            );
            assert!(
                remove(first).expect("close cross-task").is_none(),
                "cross-task close must yield None"
            );
            assert!(
                with(second, |_| ()).expect("lookup current").is_some(),
                "current task handle must still be live"
            );
            let _ = remove(second);
        });
    }
}
