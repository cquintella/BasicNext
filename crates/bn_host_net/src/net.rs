//! Interpreter-side `HOST.Net` primitives: the shared core (addresses,
//! endpoints, resolve/reverse, ping, neighbor) is `bn_rt::net` and is
//! re-exported here; this file adds the tokio-backed listener/stream/socket
//! wrappers and CIDR the interpreter provider uses.

use std::{
    io::{Read, Write},
    net::IpAddr,
};

pub use bn_rt::net::{
    Address, Endpoint, NeighborError, PingError, PingReply, ReverseError, neighbor, ping, resolve,
    resolve_timeout, reverse_timeout,
};

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
        let address = endpoint.address().as_std();
        if address.is_multicast()
            || matches!(address, IpAddr::V4(value) if value == std::net::Ipv4Addr::BROADCAST || value.octets()[3] == u8::MAX)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "multicast and broadcast datagrams are denied",
            ));
        }
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
            source: Endpoint::new(Address::from_ip(source.ip()), source.port()),
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
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
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
        let listener = self.inner.try_clone()?;
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(std::io::Error::other)?;
        runtime.block_on(async move {
            match tokio::time::timeout(timeout, async {
                tokio::net::TcpListener::from_std(listener)
                    .map_err(std::io::Error::other)?
                    .accept()
                    .await
            })
            .await
            {
                Ok(Ok((stream, _))) => {
                    let inner = stream.into_std()?;
                    inner.set_nonblocking(false)?;
                    Ok(Some(TcpStream { inner }))
                }
                Ok(Err(error)) => Err(error),
                Err(_) => Ok(None),
            }
        })
    }

    pub fn local_endpoint(&self) -> std::io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }
}

#[allow(clippy::missing_errors_doc)]
impl TcpStream {
    #[allow(dead_code)]
    #[must_use]
    pub fn from_std(inner: std::net::TcpStream) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn into_std(self) -> std::net::TcpStream {
        self.inner
    }

    pub fn connect(endpoint: Endpoint, timeout: std::time::Duration) -> std::io::Result<Self> {
        let inner = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::new(endpoint.address().as_std(), endpoint.port()),
            timeout,
        )?;
        inner.set_read_timeout(Some(timeout))?;
        inner.set_write_timeout(Some(timeout))?;
        Ok(Self { inner })
    }

    pub fn try_clone(&self) -> std::io::Result<Self> {
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
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }

    pub fn remote_endpoint(&self) -> std::io::Result<Endpoint> {
        let address = self.inner.peer_addr()?;
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }

    pub fn shutdown(&self, direction: std::net::Shutdown) -> std::io::Result<()> {
        self.inner.shutdown(direction)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// Parses and canonicalizes an address/prefix pair.
    ///
    /// # Errors
    ///
    /// Returns an error when the separator, address, or prefix is invalid.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let (address, prefix) = text.split_once('/').ok_or("CIDR requires '/'")?;
        let address = Address::parse(address)
            .map_err(|_| "invalid address")?
            .as_std();
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
        self.network == mask(address.as_std(), self.prefix)
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

#[cfg(test)]
mod tests;
