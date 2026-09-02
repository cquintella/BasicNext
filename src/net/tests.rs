use super::{Address, Cidr, Endpoint, TcpListener, TcpStream, UdpSocket};

#[test]
fn canonicalizes_and_contains_ipv4() {
    let cidr = Cidr::parse("192.168.1.9/24").expect("CIDR");
    assert_eq!(cidr.network().to_string(), "192.168.1.0");
    assert!(cidr.contains(Address::parse("192.168.1.200").expect("address")));
    assert!(!cidr.contains(Address::parse("192.168.2.1").expect("address")));
}

#[test]
fn canonicalizes_ipv6_and_rejects_invalid_prefix() {
    let cidr = Cidr::parse("2001:db8::1/64").expect("CIDR");
    assert_eq!(cidr.network().to_string(), "2001:db8::");
    assert!(Cidr::parse("10.0.0.1/33").is_err());
}

#[test]
fn resolves_unique_localhost_addresses_with_a_bound() {
    let addresses = super::resolve("localhost", 80, 1).expect("localhost resolves");
    assert_eq!(addresses.len(), 1);
}

#[test]
fn endpoint_preserves_address_and_port() {
    let endpoint = Endpoint::new(Address::parse("127.0.0.1").expect("address"), 8080);
    assert_eq!(endpoint.address().to_string(), "127.0.0.1");
    assert_eq!(endpoint.port(), 8080);
}

#[test]
fn tcp_loopback_round_trip() {
    let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind: {error}"),
    };
    let port = listener.local_addr().expect("local address").port();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut byte = [0; 1];
        std::io::Read::read_exact(&mut stream, &mut byte).expect("read");
        std::io::Write::write_all(&mut stream, &byte).expect("write");
    });
    let endpoint = Endpoint::new(Address::parse("127.0.0.1").expect("address"), port);
    let mut stream =
        TcpStream::connect(endpoint, std::time::Duration::from_secs(1)).expect("connect");
    assert_ne!(stream.local_endpoint().expect("local endpoint").port(), 0);
    assert_eq!(
        stream.remote_endpoint().expect("remote endpoint").port(),
        port
    );
    stream
        .set_timeouts(
            Some(std::time::Duration::from_secs(1)),
            Some(std::time::Duration::from_secs(1)),
        )
        .expect("timeouts");
    stream.write_bounded(b"x").expect("write");
    stream
        .shutdown(std::net::Shutdown::Write)
        .expect("shutdown write");
    let mut byte = [0; 1];
    stream.read_bounded(&mut byte).expect("read");
    assert_eq!(byte, [b'x']);
    worker.join().expect("worker");
}

#[test]
fn tcp_listener_accepts_and_reports_local_endpoint() {
    let listener = match TcpListener::bind(Endpoint::new(
        Address::parse("127.0.0.1").expect("address"),
        0,
    )) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind: {error}"),
    };
    let endpoint = listener.local_endpoint().expect("local endpoint");
    assert_ne!(endpoint.port(), 0);
    let worker = std::thread::spawn(move || {
        let mut stream = listener.accept().expect("accept");
        let mut byte = [0; 1];
        stream.read_bounded(&mut byte).expect("read");
        assert_eq!(byte, [b'z']);
    });
    let mut client =
        TcpStream::connect(endpoint, std::time::Duration::from_secs(1)).expect("connect");
    client.write_bounded(b"z").expect("write");
    worker.join().expect("worker");
}

#[test]
fn tcp_listener_bind_with_backlog_uses_bounded_socket_queue() {
    let listener = match TcpListener::bind_with_backlog(
        Endpoint::new(Address::parse("127.0.0.1").expect("address"), 0),
        8,
    ) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind with backlog: {error}"),
    };
    assert_ne!(listener.local_endpoint().expect("local endpoint").port(), 0);
}

#[test]
fn tcp_listener_accept_timeout_returns_empty() {
    let listener = match TcpListener::bind(Endpoint::new(
        Address::parse("127.0.0.1").expect("address"),
        0,
    )) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind: {error}"),
    };
    let accepted = listener
        .accept_timeout(std::time::Duration::from_millis(2))
        .expect("accept timeout");
    assert!(accepted.is_none());
}

#[test]
fn tcp_listener_accept_timeout_restores_the_accepted_stream_to_blocking() {
    let listener = match TcpListener::bind(Endpoint::new(
        Address::parse("127.0.0.1").expect("address"),
        0,
    )) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind: {error}"),
    };
    let endpoint = listener.local_endpoint().expect("local endpoint");
    let worker = std::thread::spawn(move || {
        let mut client =
            TcpStream::connect(endpoint, std::time::Duration::from_secs(1)).expect("connect");
        client.write_bounded(b"x").expect("write");
    });
    let mut stream = listener
        .accept_timeout(std::time::Duration::from_secs(1))
        .expect("accept timeout")
        .expect("accepted stream");
    let mut byte = [0; 1];
    stream.read_bounded(&mut byte).expect("blocking read");
    assert_eq!(byte, [b'x']);
    worker.join().expect("worker");
}

#[test]
fn udp_loopback_reports_source_and_payload() {
    let first = match UdpSocket::bind(Endpoint::new(
        Address::parse("127.0.0.1").expect("address"),
        0,
    )) {
        Ok(socket) => socket,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind: {error}"),
    };
    let second = UdpSocket::bind(Endpoint::new(
        Address::parse("127.0.0.1").expect("address"),
        0,
    ))
    .expect("bind second");
    assert_ne!(first.local_endpoint().expect("endpoint").port(), 0);
    let destination = second.local_endpoint().expect("endpoint");
    first.send_to(destination, b"ok").expect("send");
    let packet = second.receive(16).expect("receive");
    assert_eq!(packet.bytes(), b"ok");
    assert!(!packet.truncated());
}

#[test]
fn udp_send_rejects_broadcast_and_multicast_destinations() {
    let socket = match UdpSocket::bind(Endpoint::new(
        Address::parse("127.0.0.1").expect("address"),
        0,
    )) {
        Ok(socket) => socket,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind: {error}"),
    };
    for address in ["255.255.255.255", "192.168.1.255", "239.255.255.250"] {
        let endpoint = Endpoint::new(Address::parse(address).expect("address"), 9);
        let error = socket
            .send_to(endpoint, b"probe")
            .expect_err("restricted destination");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            error.to_string(),
            "multicast and broadcast datagrams are denied"
        );
    }
}
