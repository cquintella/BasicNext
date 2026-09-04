#![allow(dead_code)] // Provider methods are exposed through the pending C ABI.

use super::{Address, Endpoint};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr},
    time::{Duration, Instant},
};

pub struct TcpStream {
    inner: std::net::TcpStream,
}

impl TcpStream {
    pub fn connect(endpoint: Endpoint, timeout: Duration) -> io::Result<Self> {
        let address = SocketAddr::new(endpoint.address().as_std(), endpoint.port());
        let inner = std::net::TcpStream::connect_timeout(&address, timeout)?;
        inner.set_read_timeout(Some(timeout))?;
        inner.set_write_timeout(Some(timeout))?;
        Ok(Self { inner })
    }

    pub fn set_timeouts(&self, read: Duration, write: Duration) -> io::Result<()> {
        self.inner.set_read_timeout(Some(read))?;
        self.inner.set_write_timeout(Some(write))
    }

    pub fn read_bounded(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer)
    }

    pub fn write_bounded(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.inner.write(buffer)
    }

    pub fn local_endpoint(&self) -> io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }

    pub fn remote_endpoint(&self) -> io::Result<Endpoint> {
        let address = self.inner.peer_addr()?;
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }

    pub fn shutdown(&self, direction: Shutdown) -> io::Result<()> {
        self.inner.shutdown(direction)
    }
}

pub struct TcpListener {
    inner: std::net::TcpListener,
}

impl TcpListener {
    pub fn bind(endpoint: Endpoint) -> io::Result<Self> {
        Ok(Self {
            inner: std::net::TcpListener::bind(SocketAddr::new(
                endpoint.address().as_std(),
                endpoint.port(),
            ))?,
        })
    }

    pub fn bind_with_backlog(endpoint: Endpoint, backlog: usize) -> io::Result<Self> {
        let address = SocketAddr::new(endpoint.address().as_std(), endpoint.port());
        let domain = socket2::Domain::for_address(address);
        let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
        socket.set_reuse_address(true)?;
        socket.bind(&socket2::SockAddr::from(address))?;
        socket.listen(i32::try_from(backlog).map_err(|_| io::Error::other("backlog overflow"))?)?;
        Ok(Self { inner: socket.into() })
    }

    pub fn accept_timeout(&self, timeout: Duration) -> io::Result<Option<TcpStream>> {
        self.inner.set_nonblocking(true)?;
        let deadline = Instant::now() + timeout;
        let result = loop {
            match self.inner.accept() {
                Ok((stream, _)) => break Ok(Some(TcpStream { inner: stream })),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        break Ok(None);
                    }
                    std::thread::yield_now();
                }
                Err(error) => break Err(error),
            }
        };
        self.inner.set_nonblocking(false)?;
        result
    }

    pub fn local_endpoint(&self) -> io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }
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
    pub fn bind(endpoint: Endpoint) -> io::Result<Self> {
        Ok(Self {
            inner: std::net::UdpSocket::bind(SocketAddr::new(
                endpoint.address().as_std(),
                endpoint.port(),
            ))?,
        })
    }

    pub fn set_read_timeout(&self, timeout: Duration) -> io::Result<()> {
        self.inner.set_read_timeout(Some(timeout))
    }

    pub fn send_to(&self, endpoint: Endpoint, bytes: &[u8]) -> io::Result<usize> {
        let address = endpoint.address().as_std();
        if address.is_multicast()
            || matches!(address, std::net::IpAddr::V4(value) if value.octets()[3] == u8::MAX)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "multicast and broadcast datagrams are denied",
            ));
        }
        self.inner.send_to(
            bytes,
            SocketAddr::new(endpoint.address().as_std(), endpoint.port()),
        )
    }

    pub fn receive(&self, maximum: usize) -> io::Result<UdpPacket> {
        let mut bytes = vec![0; maximum];
        let (count, source) = self.inner.recv_from(&mut bytes)?;
        bytes.truncate(count);
        Ok(UdpPacket {
            source: Endpoint::new(Address::from_ip(source.ip()), source.port()),
            bytes,
            truncated: count == maximum,
        })
    }

    pub fn local_endpoint(&self) -> io::Result<Endpoint> {
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
