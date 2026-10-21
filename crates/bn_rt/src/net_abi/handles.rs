use crate::net::{TcpListener, TcpStream, UdpPacket, UdpSocket};
use std::sync::{Arc, Mutex};

pub enum Handle {
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
    UdpPacket(UdpPacket),
}

/// The sockets of the module copy that runs on this thread (`bn_core_copy`):
/// a task's close when the task ends.
#[derive(Default)]
struct Sockets(Mutex<Vec<Option<Handle>>>);

fn table() -> Arc<Sockets> {
    bn_core_copy::table::<Sockets>()
}

pub fn insert(handle: Handle) -> Result<usize, &'static str> {
    let sockets = table();
    let mut table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    if let Some((index, slot)) = table
        .iter_mut()
        .enumerate()
        .find(|(_, slot)| slot.is_none())
    {
        *slot = Some(handle);
        return Ok(index);
    }
    // The quota the interpreter applies too (`bn_limits`).
    if table.len() >= bn_limits::web_limits().socket_handles_max {
        return Err("socket handle quota exceeded");
    }
    table.push(Some(handle));
    Ok(table.len() - 1)
}

pub fn remove(index: usize) -> Result<Option<Handle>, &'static str> {
    let sockets = table();
    let mut table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get_mut(index).and_then(Option::take))
}

pub fn with<T>(
    index: usize,
    operation: impl FnOnce(&Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let sockets = table();
    let table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get(index).and_then(Option::as_ref).map(operation))
}

pub(crate) fn with_mut<T>(
    index: usize,
    operation: impl FnOnce(&mut Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let sockets = table();
    let mut table = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get_mut(index).and_then(Option::as_mut).map(operation))
}

#[cfg(test)]
mod tests {
    use super::{Handle, insert, remove, with};
    use crate::net::{Address, Endpoint, UdpSocket};

    #[test]
    fn handles_are_reusable_after_close() {
        let _lock = crate::network_test_lock();
        let socket = UdpSocket::bind(Endpoint::new(
            Address::parse("127.0.0.1").expect("loopback"),
            0,
        ))
        .expect("bind loopback");
        let first = insert(Handle::UdpSocket(socket)).expect("insert handle");
        assert!(with(first, |_| ()).expect("lookup").is_some());
        let _ = remove(first).expect("remove handle");
        let socket = UdpSocket::bind(Endpoint::new(
            Address::parse("127.0.0.1").expect("loopback"),
            0,
        ))
        .expect("bind loopback");
        let second = insert(Handle::UdpSocket(socket)).expect("reuse handle");
        assert!(
            with(second, |_| ())
                .expect("lookup reused handle")
                .is_some()
        );
        let _ = remove(second);
    }
}
