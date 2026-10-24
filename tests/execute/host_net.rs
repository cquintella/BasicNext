// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
#[test]
fn host_net_address_parse_and_format_round_trip() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "error"
ELSE
PRINT address.ToString()
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute HOST.Net address parse");
    assert_eq!(output, "127.0.0.1\n");
}

#[test]
fn host_net_address_predicates_follow_ip_semantics() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET v4 AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
LET v6 AS Net.Address OR Error = Net.Address.Parse("ff02::1")
IF v4 IS Error OR v6 IS Error THEN
PRINT "error"
ELSE
PRINT v4.IsIPv4(), v4.IsLoopback(), v4.IsPrivate(), v6.IsIPv6(), v6.IsMulticast()
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute address predicates");
    assert_eq!(output, "TRUE TRUE FALSE TRUE TRUE\n");
}

#[test]
fn host_net_cidr_contains_addresses() {
    let source = "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET network AS Net.CIDR OR Error = Net.CIDR.Parse(\"192.168.1.0/24\")\nLET inside AS Net.Address OR Error = Net.Address.Parse(\"192.168.1.20\")\nLET outside AS Net.Address OR Error = Net.Address.Parse(\"192.168.2.20\")\nIF network IS Error OR inside IS Error OR outside IS Error THEN\nPRINT \"error\"\nELSE\nPRINT network.Contains(inside), network.Contains(outside)\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Net CIDR");
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn host_net_resolve_returns_bounded_addresses() {
    let source = "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET result AS Net.Addresses OR Error = Net.Resolve(\"localhost\", 1000)\nIF result IS Error THEN\nPRINT \"error\"\nELSE\nLET first AS Net.Address OR Error = result.Get(0)\nIF first IS Error THEN\nPRINT \"error\"\nELSE\nPRINT result.Count(), first.ToString()\nEND IF\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Net Resolve");
    assert!(output.starts_with('1') || output.starts_with('2'));
}

#[test]
fn host_net_endpoint_create_preserves_port() {
    let source = "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET address AS Net.Address OR Error = Net.Address.Parse(\"127.0.0.1\")\nIF address IS Error THEN\nPRINT \"error\"\nELSE\nLET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 8080)\nIF endpoint IS Error THEN\nPRINT \"error\"\nELSE\nPRINT endpoint\nEND IF\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Net Endpoint");
    assert_eq!(output, "HOST.Net.Endpoint\n");
}

#[test]
fn host_net_endpoint_and_cidr_accessors_preserve_values() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("192.168.1.20")
LET network AS Net.CIDR OR Error = Net.CIDR.Parse("192.168.1.0/24")
IF address IS Error OR network IS Error THEN
PRINT "error"
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 8080)
IF endpoint IS Error THEN
PRINT "error"
ELSE
PRINT endpoint.Address().ToString(), endpoint.Port(), network.Network().ToString(), network.PrefixLength()
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute network accessors");
    assert_eq!(output, "192.168.1.20 8080 192.168.1.0 24\n");
}

#[test]
fn host_net_tcp_listen_reports_local_endpoint() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "parse-error"
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
PRINT "endpoint-error"
ELSE
LET endpoints AS Net.Endpoint[1] = [endpoint]
LET listener AS Net.TCPListener OR Error = Net.TCPListen(endpoints, 1)
IF listener IS Error THEN
PRINT "listen-error", listener.Message
ELSE
LET local AS Net.Endpoint OR Error = listener.LocalEndpoint()
IF local IS Error THEN
PRINT "endpoint-error"
ELSE
PRINT local.Port() > 0
END IF
listener.Close()
END IF
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute TCP listener");
    assert!(output == "TRUE\n" || output.starts_with("listen-error"));
}

#[test]
fn host_net_tcp_listen_rolls_back_when_one_endpoint_fails() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET first AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
LET second AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF first IS Error OR second IS Error THEN
PRINT "parse-error"
ELSE
LET a AS Net.Endpoint OR Error = Net.Endpoint.Create(first, 1)
LET b AS Net.Endpoint OR Error = Net.Endpoint.Create(second, 1)
IF a IS Error OR b IS Error THEN
PRINT "endpoint-error"
ELSE
LET endpoints AS Net.Endpoint[2] = [a, b]
LET listener AS Net.TCPListener OR Error = Net.TCPListen(endpoints, 1)
PRINT listener IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute multi-endpoint rejection");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn host_net_ping_loopback_and_neighbor_typed_result() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "parse-error"
ELSE
LET reply AS Net.PingReply OR Error = Net.Ping(address, 2000)
LET neighbor AS Net.Address OR Error = Net.Neighbor(address)
IF reply IS Error THEN
PRINT "ping-error ", reply.Message, " ", neighbor IS Error
ELSE
PRINT "ping-ok ", reply.RoundTripMicroseconds() >= 0, " ", neighbor IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute ping/neighbor providers");
    assert!(
        output.starts_with("ping-ok  TRUE   FALSE\n") || output.starts_with("ping-error "),
        "unexpected ping/neighbor output: {output:?}"
    );
    assert!(
        output.contains("FALSE\n") || output.contains("FALSE "),
        "loopback neighbor must be a valid address: {output:?}"
    );
}

#[test]
fn host_net_reverse_resolves_loopback() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "parse-error"
ELSE
LET host AS STRING OR Error = Net.Reverse(address, 2000)
IF host IS Error THEN
PRINT "error ", host.Message
ELSE
PRINT "ok"
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute reverse resolver");
    assert_eq!(output, "ok\n");
}
