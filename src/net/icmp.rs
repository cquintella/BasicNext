//! Bounded ICMP Echo Request/Reply for HOST.Net.Ping.

#![allow(unsafe_code)] // recv_from fills MaybeUninit; only the returned length is read.

use std::{
    io::{self, ErrorKind},
    mem::MaybeUninit,
    net::IpAddr,
    time::{Duration, Instant},
};

use socket2::{Domain, Protocol, Socket, Type};

use super::Address;

const PAYLOAD: [u8; 32] = *b"BasicNextICMPEchoPayload!!!!!!!!";

#[derive(Debug)]
pub enum PingError {
    Timeout,
    Unreachable,
    PermissionDenied,
    Unavailable,
    Io(io::Error),
}

impl PingError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Timeout => "ping timeout".into(),
            Self::Unreachable => "ping destination unreachable".into(),
            Self::PermissionDenied => "ICMP Echo permission denied".into(),
            Self::Unavailable => "ICMP Echo operation unavailable on this host".into(),
            Self::Io(error) => error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PingReply {
    pub address: Address,
    pub round_trip_microseconds: i64,
}

/// Sends one Echo Request and waits up to `timeout` for a matching Reply.
///
/// # Errors
///
/// Returns typed ping failures or the underlying I/O error.
pub fn ping(address: Address, timeout: Duration) -> Result<PingReply, PingError> {
    match address.as_std() {
        IpAddr::V4(v4) => ping_v4(v4, timeout),
        IpAddr::V6(v6) => ping_v6(v6, timeout),
    }
}

fn ping_v4(dest: std::net::Ipv4Addr, timeout: Duration) -> Result<PingReply, PingError> {
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).map_err(map_open)?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(PingError::Io)?;
    let id = (std::process::id() & 0xffff) as u16;
    let seq = 1u16;
    let mut packet = [0u8; 8 + PAYLOAD.len()];
    packet[0] = 8; // Echo Request
    packet[4..6].copy_from_slice(&id.to_be_bytes());
    packet[6..8].copy_from_slice(&seq.to_be_bytes());
    packet[8..].copy_from_slice(&PAYLOAD);
    let checksum = icmp_checksum(&packet);
    packet[2..4].copy_from_slice(&checksum.to_be_bytes());
    let start = Instant::now();
    socket
        .send_to(
            &packet,
            &socket2::SockAddr::from(std::net::SocketAddr::new(IpAddr::V4(dest), 0)),
        )
        .map_err(map_send)?;
    let mut buffer = [MaybeUninit::uninit(); 1500];
    loop {
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(PingError::Timeout);
        }
        socket
            .set_read_timeout(Some(remaining))
            .map_err(PingError::Io)?;
        let (received, from) = match socket.recv_from(&mut buffer) {
            Ok(value) => value,
            Err(error)
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
            {
                return Err(PingError::Timeout);
            }
            Err(error) => return Err(map_recv(error)),
        };
        if received < 8 {
            continue;
        }
        // DGRAM ICMP replies often omit the IP header.
        let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), received) };
        let Some(icmp) = ipv4_icmp_payload(bytes) else {
            continue;
        };
        if icmp[0] != 0 {
            continue;
        }
        let identifier = u16::from_be_bytes([icmp[4], icmp[5]]);
        let reply_seq = u16::from_be_bytes([icmp[6], icmp[7]]);
        if identifier != id || reply_seq != seq {
            continue;
        }
        let micros = i64::try_from(start.elapsed().as_micros()).unwrap_or(i64::MAX);
        let address_ip = from
            .as_socket_ipv4()
            .map(|addr| IpAddr::V4(*addr.ip()))
            .or_else(|| from.as_socket().map(|addr| addr.ip()))
            .unwrap_or(IpAddr::V4(dest));
        return Ok(PingReply {
            address: Address::from_ip(address_ip),
            round_trip_microseconds: micros,
        });
    }
}

fn ping_v6(dest: std::net::Ipv6Addr, timeout: Duration) -> Result<PingReply, PingError> {
    let socket =
        Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::ICMPV6)).map_err(map_open)?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(PingError::Io)?;
    let id = (std::process::id() & 0xffff) as u16;
    let seq = 1u16;
    let mut packet = [0u8; 8 + PAYLOAD.len()];
    packet[0] = 128; // Echo Request
    packet[4..6].copy_from_slice(&id.to_be_bytes());
    packet[6..8].copy_from_slice(&seq.to_be_bytes());
    packet[8..].copy_from_slice(&PAYLOAD);
    // ICMPv6 checksum is typically offloaded for DGRAM sockets.
    let start = Instant::now();
    socket
        .send_to(
            &packet,
            &socket2::SockAddr::from(std::net::SocketAddr::new(IpAddr::V6(dest), 0)),
        )
        .map_err(map_send)?;
    let mut buffer = [MaybeUninit::uninit(); 1500];
    loop {
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(PingError::Timeout);
        }
        socket
            .set_read_timeout(Some(remaining))
            .map_err(PingError::Io)?;
        let (received, from) = match socket.recv_from(&mut buffer) {
            Ok(value) => value,
            Err(error)
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
            {
                return Err(PingError::Timeout);
            }
            Err(error) => return Err(map_recv(error)),
        };
        let icmp = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), received) };
        if received < 8 || icmp[0] != 129 {
            continue;
        }
        let identifier = u16::from_be_bytes([icmp[4], icmp[5]]);
        let reply_seq = u16::from_be_bytes([icmp[6], icmp[7]]);
        if identifier != id || reply_seq != seq {
            continue;
        }
        let micros = i64::try_from(start.elapsed().as_micros()).unwrap_or(i64::MAX);
        let address_ip = from
            .as_socket_ipv6()
            .map(|addr| IpAddr::V6(*addr.ip()))
            .or_else(|| from.as_socket().map(|addr| addr.ip()))
            .unwrap_or(IpAddr::V6(dest));
        return Ok(PingReply {
            address: Address::from_ip(address_ip),
            round_trip_microseconds: micros,
        });
    }
}

fn ipv4_icmp_payload(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 8 {
        return None;
    }
    // Some stacks deliver Echo Reply with an IPv4 header; DGRAM often omits it.
    if bytes[0] >> 4 == 4 {
        let header_len = usize::from(bytes[0] & 0x0f) * 4;
        if bytes.len() < header_len + 8 {
            return None;
        }
        return Some(&bytes[header_len..]);
    }
    Some(bytes)
}

fn icmp_checksum(packet: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut chunks = packet.chunks_exact(2);
    for chunk in chunks.by_ref() {
        sum += u32::from(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    if let Some(&byte) = chunks.remainder().first() {
        sum += u32::from(byte) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn map_open(error: io::Error) -> PingError {
    match error.kind() {
        ErrorKind::PermissionDenied => PingError::PermissionDenied,
        ErrorKind::Unsupported | ErrorKind::Other if cfg!(target_os = "macos") => {
            PingError::Unavailable
        }
        _ => {
            if error.raw_os_error() == Some(libc_eperm()) {
                PingError::PermissionDenied
            } else if cfg!(target_os = "macos") {
                PingError::Unavailable
            } else {
                PingError::Io(error)
            }
        }
    }
}

fn map_send(error: io::Error) -> PingError {
    match error.kind() {
        ErrorKind::PermissionDenied => PingError::PermissionDenied,
        ErrorKind::NetworkUnreachable | ErrorKind::HostUnreachable => PingError::Unreachable,
        _ => PingError::Io(error),
    }
}

fn map_recv(error: io::Error) -> PingError {
    match error.kind() {
        ErrorKind::NetworkUnreachable | ErrorKind::HostUnreachable => PingError::Unreachable,
        ErrorKind::PermissionDenied => PingError::PermissionDenied,
        _ => PingError::Io(error),
    }
}

fn libc_eperm() -> i32 {
    1 // EPERM on Unix
}
