//! Interpreter-side `HOST.Net` primitives: all of them are the shared core,
//! `bn_rt::net`, re-exported for the provider and `BNWeb`.

pub use bn_rt::net::{
    Address, Cidr, Endpoint, NeighborError, PingError, PingReply, ReverseError, TcpListener,
    TcpStream, UdpPacket, UdpSocket, neighbor, ping, resolve, resolve_timeout, reverse_timeout,
};

#[cfg(test)]
mod tests;
