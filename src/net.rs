use std::{
    collections::HashSet,
    fmt,
    io::{Read, Write},
    net::{IpAddr, ToSocketAddrs},
    str::FromStr,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Address(IpAddr);

impl Address {
    /// Parses a strict IPv4 or IPv6 address.
    ///
    /// # Errors
    ///
    /// Returns the standard parser error when `text` is not an address.
    pub fn parse(text: &str) -> Result<Self, std::net::AddrParseError> {
        text.parse().map(Self)
    }

    #[must_use]
    pub const fn as_std(self) -> IpAddr {
        self.0
    }
}

#[derive(Debug)]
pub struct TcpStream {
    inner: std::net::TcpStream,
}

pub struct UdpSocket {
    inner: std::net::UdpSocket,
}

pub struct UdpPacket {
    source: Endpoint,
    bytes: Vec<u8>,
    truncated: bool,
}

impl UdpSocket {
    #[allow(clippy::missing_errors_doc)]
    pub fn bind(endpoint: Endpoint) -> std::io::Result<Self> {
        Ok(Self {
            inner: std::net::UdpSocket::bind(std::net::SocketAddr::new(
                endpoint.address().as_std(),
                endpoint.port(),
            ))?,
        })
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn send_to(&self, endpoint: Endpoint, bytes: &[u8]) -> std::io::Result<usize> {
        self.inner.send_to(
            bytes,
            std::net::SocketAddr::new(endpoint.address().as_std(), endpoint.port()),
        )
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn receive(&self, maximum: usize) -> std::io::Result<UdpPacket> {
        let mut bytes = vec![0; maximum];
        let (count, source) = self.inner.recv_from(&mut bytes)?;
        bytes.truncate(count);
        Ok(UdpPacket {
            source: Endpoint::new(Address(source.ip()), source.port()),
            truncated: count == maximum,
            bytes,
        })
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        self.inner.set_read_timeout(timeout)
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn local_endpoint(&self) -> std::io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(Address(address.ip()), address.port()))
    }
}

impl UdpPacket {
    #[must_use]
    pub const fn source(&self) -> Endpoint {
        self.source
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

pub struct TcpListener {
    inner: std::net::TcpListener,
}

#[allow(clippy::missing_errors_doc)]
impl TcpListener {
    pub fn bind(endpoint: Endpoint) -> std::io::Result<Self> {
        Ok(Self {
            inner: std::net::TcpListener::bind(std::net::SocketAddr::new(
                endpoint.address().as_std(),
                endpoint.port(),
            ))?,
        })
    }

    pub fn bind_with_backlog(endpoint: Endpoint, backlog: usize) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .map_err(std::io::Error::other)?;
        runtime
            .block_on(async move {
                let socket = match endpoint.address().as_std() {
                    IpAddr::V4(_) => tokio::net::TcpSocket::new_v4()?,
                    IpAddr::V6(_) => tokio::net::TcpSocket::new_v6()?,
                };
                socket.set_reuseaddr(true)?;
                socket.bind(std::net::SocketAddr::new(
                    endpoint.address().as_std(),
                    endpoint.port(),
                ))?;
                let listener = socket.listen(u32::try_from(backlog).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "backlog is too large")
                })?)?;
                listener.into_std()
            })
            .map(|inner| Self { inner })
    }

    pub fn accept(&self) -> std::io::Result<TcpStream> {
        let (inner, _) = self.inner.accept()?;
        Ok(TcpStream { inner })
    }

    pub fn accept_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> std::io::Result<Option<TcpStream>> {
        self.inner.set_nonblocking(true)?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.accept() {
                Ok(stream) => {
                    stream.inner.set_nonblocking(false)?;
                    self.inner.set_nonblocking(false)?;
                    return Ok(Some(stream));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        self.inner.set_nonblocking(false)?;
                        return Ok(None);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(error) => {
                    let _ = self.inner.set_nonblocking(false);
                    return Err(error);
                }
            }
        }
    }

    pub fn local_endpoint(&self) -> std::io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(Address(address.ip()), address.port()))
    }
}

#[allow(clippy::missing_errors_doc)]
impl TcpStream {
    #[allow(dead_code)]
    pub(crate) fn from_std(inner: std::net::TcpStream) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn into_std(self) -> std::net::TcpStream {
        self.inner
    }

    pub fn connect(endpoint: Endpoint, timeout: std::time::Duration) -> std::io::Result<Self> {
        Ok(Self {
            inner: std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::new(endpoint.address().as_std(), endpoint.port()),
                timeout,
            )?,
        })
    }

    pub(crate) fn try_clone(&self) -> std::io::Result<Self> {
        Ok(Self {
            inner: self.inner.try_clone()?,
        })
    }

    pub fn set_timeouts(
        &self,
        read: Option<std::time::Duration>,
        write: Option<std::time::Duration>,
    ) -> std::io::Result<()> {
        self.inner.set_read_timeout(read)?;
        self.inner.set_write_timeout(write)
    }

    pub fn read_bounded(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }

    pub fn write_bounded(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buffer)
    }

    pub fn local_endpoint(&self) -> std::io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(Address(address.ip()), address.port()))
    }

    pub fn remote_endpoint(&self) -> std::io::Result<Endpoint> {
        let address = self.inner.peer_addr()?;
        Ok(Endpoint::new(Address(address.ip()), address.port()))
    }

    pub fn shutdown(&self, direction: std::net::Shutdown) -> std::io::Result<()> {
        self.inner.shutdown(direction)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

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

impl Cidr {
    /// Parses and canonicalizes an address/prefix pair.
    ///
    /// # Errors
    ///
    /// Returns an error when the separator, address, or prefix is invalid.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let (address, prefix) = text.split_once('/').ok_or("CIDR requires '/'")?;
        let address = Address::parse(address).map_err(|_| "invalid address")?.0;
        let prefix = prefix.parse::<u8>().map_err(|_| "invalid prefix")?;
        let maximum = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > maximum {
            return Err("prefix is wider than address family");
        }
        Ok(Self {
            network: mask(address, prefix),
            prefix,
        })
    }

    #[must_use]
    pub const fn network(self) -> IpAddr {
        self.network
    }

    #[must_use]
    pub const fn prefix_length(self) -> u8 {
        self.prefix
    }

    #[must_use]
    pub fn contains(self, address: Address) -> bool {
        self.network == mask(address.0, self.prefix)
    }
}

fn mask(address: IpAddr, prefix: u8) -> IpAddr {
    match address {
        IpAddr::V4(value) => IpAddr::V4(std::net::Ipv4Addr::from(
            u32::from(value)
                & if prefix == 0 {
                    0
                } else {
                    !0u32 << (32 - u32::from(prefix))
                },
        )),
        IpAddr::V6(value) => IpAddr::V6(std::net::Ipv6Addr::from(
            u128::from(value)
                & if prefix == 0 {
                    0
                } else {
                    !0u128 << (128 - u32::from(prefix))
                },
        )),
    }
}

impl FromStr for Address {
    type Err = std::net::AddrParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Resolves a host through the operating system and returns bounded unique addresses.
///
/// # Errors
///
/// Returns the operating-system resolution error when no address can be resolved.
pub fn resolve(host: &str, port: u16, maximum: usize) -> std::io::Result<Vec<Address>> {
    if maximum == 0 {
        return Ok(Vec::new());
    }
    let mut seen = HashSet::new();
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

/// Resolves with a bounded wait. The OS resolver thread may finish after a timeout.
///
/// # Errors
///
/// Returns resolver/provider errors reported by the operating system.
pub fn resolve_timeout(
    host: &str,
    port: u16,
    maximum: usize,
    timeout: std::time::Duration,
) -> std::io::Result<Option<Vec<Address>>> {
    // ponytail: detached resolver thread bounds caller wait; replace with cancellable OS API if available.
    let host = host.to_owned();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(resolve(&host, port, maximum));
    });
    match receiver.recv_timeout(timeout) {
        Ok(result) => result.map(Some),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(std::io::Error::other("resolver provider stopped"))
        }
    }
}

#[cfg(test)]
mod tests;
