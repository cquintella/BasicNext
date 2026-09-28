// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` sockets: TCP streams and listeners, UDP sockets and packets.
//! One implementation for the interpreter provider, `BNWeb`, and the C ABI.
//! Every fallible method returns the operating-system error unchanged.
#![allow(clippy::missing_errors_doc)]

use super::{Address, Endpoint};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr},
    time::{Duration, Instant},
};

/// The largest UDP payload (IPv6 without jumbograms is 65 527 bytes).
const UDP_PAYLOAD_MAX: usize = 65_536;

#[derive(Debug)]
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

    /// Wraps a connected standard stream (`BNWeb` hands streams back).
    #[must_use]
    pub const fn from_std(inner: std::net::TcpStream) -> Self {
        Self { inner }
    }

    /// The standard stream, for `BNWeb`'s HTTP layer.
    #[must_use]
    pub fn into_std(self) -> std::net::TcpStream {
        self.inner
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            inner: self.inner.try_clone()?,
        })
    }

    /// Read and write timeouts; `None` waits without bound.
    pub fn set_timeouts(&self, read: Option<Duration>, write: Option<Duration>) -> io::Result<()> {
        self.inner.set_read_timeout(read)?;
        self.inner.set_write_timeout(write)
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

#[derive(Debug)]
pub struct TcpListener {
    inner: std::net::TcpListener,
}

impl TcpListener {
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            inner: self.inner.try_clone()?,
        })
    }

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
        let socket =
            socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
        // As `std::net::TcpListener::bind`: SO_REUSEADDR only on Unix, where
        // it skips TIME_WAIT. On Windows it lets a second socket bind a port
        // already in use, so `Net.ADDRESS_IN_USE` would never be reported.
        #[cfg(not(windows))]
        socket.set_reuse_address(true)?;
        socket.bind(&socket2::SockAddr::from(address))?;
        socket.listen(i32::try_from(backlog).map_err(|_| io::Error::other("backlog overflow"))?)?;
        Ok(Self {
            inner: socket.into(),
        })
    }

    /// Waits for one connection without bound.
    pub fn accept(&self) -> io::Result<TcpStream> {
        let (inner, _) = self.inner.accept()?;
        Ok(TcpStream { inner })
    }

    /// Waits at most `timeout` for one connection; `None` when none came.
    /// Polls a non-blocking clone every millisecond (no `tokio` in the
    /// native runtime, and no busy spin).
    pub fn accept_timeout(&self, timeout: Duration) -> io::Result<Option<TcpStream>> {
        let listener = self.inner.try_clone()?;
        listener.set_nonblocking(true)?;
        let deadline = Instant::now() + timeout;
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false)?;
                    return Ok(Some(TcpStream { inner: stream }));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(None);
                    }
                    std::thread::sleep((deadline - now).min(Duration::from_millis(1)));
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn local_endpoint(&self) -> io::Result<Endpoint> {
        let address = self.inner.local_addr()?;
        Ok(Endpoint::new(
            Address::from_ip(address.ip()),
            address.port(),
        ))
    }
}

#[derive(Debug)]
pub struct UdpSocket {
    inner: std::net::UdpSocket,
}

#[derive(Debug)]
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

    /// Receive timeout; `None` waits without bound.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_read_timeout(timeout)
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

    /// One datagram, cut to `maximum` bytes. The buffer holds any UDP
    /// payload, so truncation is exact on every platform: Unix would drop the
    /// excess silently, and Windows fails the call (`WSAEMSGSIZE`) and loses
    /// the source endpoint.
    // ponytail: 64 KiB per receive; reuse a per-socket buffer if receive
    // rates make the allocation show up.
    pub fn receive(&self, maximum: usize) -> io::Result<UdpPacket> {
        let mut bytes = vec![0; UDP_PAYLOAD_MAX.max(maximum)];
        let (count, source) = self.inner.recv_from(&mut bytes)?;
        bytes.truncate(count.min(maximum));
        Ok(UdpPacket {
            source: Endpoint::new(Address::from_ip(source.ip()), source.port()),
            bytes,
            truncated: count > maximum,
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
