#![allow(dead_code)] // TCP/UDP providers are consumed by the C ABI next.

//! HOST.Net providers shared by the interpreter and compiled binaries.

pub(crate) mod handles;
mod icmp;
mod neighbor;
mod reverse;
mod socket;

#[allow(unused_imports)]
pub use socket::{TcpListener, TcpStream, UdpPacket, UdpSocket};

use std::{
    net::{IpAddr, ToSocketAddrs},
    str::FromStr,
};

pub use icmp::{PingError, PingReply, ping};
pub use neighbor::{NeighborError, neighbor};
pub use reverse::{ReverseError, reverse_timeout};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Endpoint {
    address: Address,
    port: u16,
}

impl Endpoint {
    #[must_use]
    pub const fn new(address: Address, port: u16) -> Self {
        Self { address, port }
    }

    #[must_use]
    pub const fn address(self) -> Address {
        self.address
    }

    #[must_use]
    pub const fn port(self) -> u16 {
        self.port
    }
}

/// Opaque compiled-runtime result for HOST.Net.Resolve.
///
/// The LLVM ABI owns this allocation until `bn_rt_net_addresses_free` is called.
pub struct AddressesHandle {
    values: Vec<Address>,
}

fn resolver_tasks() -> &'static std::sync::Mutex<Vec<std::thread::JoinHandle<()>>> {
    static TASKS: std::sync::OnceLock<std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>> =
        std::sync::OnceLock::new();
    TASKS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

pub(crate) fn retain_resolver_task(task: std::thread::JoinHandle<()>) {
    resolver_tasks()
        .lock()
        .expect("resolver task registry poisoned")
        .push(task);
}

/// Joins retained reverse/resolve workers.
///
/// # Panics
///
/// Panics if the resolver-task registry mutex is poisoned.
pub fn join_resolver_tasks() {
    let tasks = resolver_tasks();
    let mut tasks = tasks.lock().expect("resolver task registry poisoned");
    while let Some(task) = tasks.pop() {
        let _ = task.join();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Address(IpAddr);

impl Address {
    /// Parses a strict IPv4 or IPv6 address.
    ///
    /// # Errors
    ///
    /// Returns the parser error when `text` is not an address.
    pub fn parse(text: &str) -> Result<Self, std::net::AddrParseError> {
        text.parse().map(Self)
    }

    #[must_use]
    pub const fn from_ip(address: IpAddr) -> Self {
        Self(address)
    }

    #[must_use]
    pub const fn as_std(self) -> IpAddr {
        self.0
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for Address {
    type Err = std::net::AddrParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Resolves a host name to unique addresses (forward lookup helper for C ABI).
///
/// # Errors
///
/// Returns the OS resolution error.
#[allow(dead_code)]
pub fn resolve(host: &str, port: u16, maximum: usize) -> std::io::Result<Vec<Address>> {
    if maximum == 0 {
        return Ok(Vec::new());
    }
    let mut seen = std::collections::HashSet::new();
    let mut addresses = Vec::new();
    for endpoint in (host, port).to_socket_addrs()? {
        let address = Address(endpoint.ip());
        if seen.insert(address) {
            addresses.push(address);
            if addresses.len() == maximum {
                break;
            }
        }
    }
    Ok(addresses)
}

/// Resolves with a bounded worker and timeout.
pub fn resolve_timeout(
    host: &str,
    port: u16,
    maximum: usize,
    timeout: std::time::Duration,
) -> std::io::Result<Option<Vec<Address>>> {
    let host = host.to_owned();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let task = std::thread::spawn(move || {
        let _ = sender.send(resolve(&host, port, maximum));
    });
    match receiver.recv_timeout(timeout) {
        Ok(result) => {
            let _ = task.join();
            result.map(Some)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            retain_resolver_task(task);
            Ok(None)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            let _ = task.join();
            Err(std::io::Error::other("resolver worker stopped"))
        }
    }
}

impl AddressesHandle {
    /// Resolves at most `maximum` unique addresses.
    ///
    /// # Errors
    ///
    /// Returns the operating-system resolver error.
    pub fn resolve(host: &str, port: u16, maximum: usize) -> std::io::Result<Self> {
        Ok(Self {
            values: resolve(host, port, maximum)?,
        })
    }

    /// Resolves at most `maximum` addresses before the timeout expires.
    ///
    /// # Errors
    ///
    /// Returns the resolver error; `Ok(None)` indicates timeout.
    pub fn resolve_timeout(
        host: &str,
        port: u16,
        maximum: usize,
        timeout: std::time::Duration,
    ) -> std::io::Result<Option<Self>> {
        Ok(resolve_timeout(host, port, maximum, timeout)?.map(|values| Self { values }))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<Address> {
        self.values.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::AddressesHandle;

    #[test]
    fn resolve_handle_honors_zero_bound_without_network_access() {
        let handle = AddressesHandle::resolve("invalid.invalid", 80, 0)
            .expect("zero bound must not resolve");
        assert!(handle.is_empty());
        assert_eq!(handle.len(), 0);
        assert_eq!(handle.get(0), None);
    }
}
