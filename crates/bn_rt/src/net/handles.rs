use super::{TcpListener, TcpStream, UdpPacket, UdpSocket};
use std::sync::{Mutex, OnceLock};

const MAX_HANDLES: usize = 256;

pub enum Handle {
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
    UdpPacket(UdpPacket),
}

fn table() -> &'static Mutex<Vec<Option<Handle>>> {
    static TABLE: OnceLock<Mutex<Vec<Option<Handle>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn insert(handle: Handle) -> Result<usize, &'static str> {
    let mut table = table()
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
    if table.len() == MAX_HANDLES {
        return Err("socket handle quota exceeded");
    }
    table.push(Some(handle));
    Ok(table.len() - 1)
}

pub fn remove(index: usize) -> Result<Option<Handle>, &'static str> {
    let mut table = table()
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get_mut(index).and_then(Option::take))
}

pub fn with<T>(
    index: usize,
    operation: impl FnOnce(&Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let table = table()
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(table.get(index).and_then(Option::as_ref).map(operation))
}

pub fn with_mut<T>(
    index: usize,
    operation: impl FnOnce(&mut Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let mut table = table()
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
