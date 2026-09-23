# HOST.Net Capability 0.3

## Status

Accepted 0.3 functional scope. Phase 0 freezes exact type and method signatures,
ownership, error codes, limits, and provider availability before implementation.

`HOST.Net` is an explicitly imported host capability:

```basic
IMPORT HOST.Net AS Net
```

It is the only network provider used by `BNWeb`. `BNWeb` may use an internal
adapter over resolved `HOST.Net` provider handles, but it must not create a
parallel resolver, socket provider, or host capability.

## Values

The capability provides unforgeable host values for:

- IPv4 and IPv6 addresses;
- CIDR prefixes;
- transport endpoints;
- bounded resolver-result collections;
- TCP streams and listeners;
- UDP sockets;
- ICMP Echo replies.

Variable-size results use opaque collections with `Count` and `Get`. They are
not variable-size BN vectors. No value exposes a file descriptor, native
handle, raw packet, or mutable operating-system network structure.

## Addressing and resolution

0.3 includes:

- strict IPv4/IPv6 parse and canonical formatting;
- address-family and loopback/private/link-local/multicast tests;
- CIDR construction, canonical network address, prefix length, and containment;
- endpoint construction from address and port;
- bounded system forward resolution;
- bounded system reverse resolution.

Resolution preserves provider order, removes exact duplicate addresses, and
returns a typed empty-result or provider error. A hostname is not silently
resolved by an operation whose contract requires an address.

## TCP

0.3 includes bounded connect, bind, listen, accept, read, write, endpoint
inspection, timeout configuration, half/full shutdown, and idempotent close.
Partial reads/writes and EOF are explicit outcomes. Dual-stack startup is
all-or-nothing for every listener set requested as one operation; a listener
set contains at most 16 endpoints.

Safe typed options may cover backlog, `TCP_NODELAY`, keepalive, and read/write
timeouts. BN does not expose numeric `setsockopt`, descriptor duplication, or
platform-specific socket constants.

## UDP

0.3 includes bounded bind, send-to, receive-from, endpoint inspection, timeout,
and idempotent close. Every received datagram reports its source endpoint.
Truncation is an explicit result; it must not be reported as a complete
datagram. Arbitrary raw IP packets are outside 0.3.

## ICMP Echo

`Ping` sends one IPv4 or IPv6 Echo Request to an already parsed address and
waits for at most the supplied bounded timeout. The host generates the
identifier, sequence, and fixed 32-byte payload. A successful reply reports
the responding address and round-trip time in microseconds.

Timeout, unreachable, permission-denied, and operation-unavailable are distinct
typed errors. Missing ICMP permission disables only `Ping`; it does not disable
addressing, resolution, TCP, UDP, or the complete `HOST.Net` import.

Caller-controlled ICMP payloads, arbitrary ICMP types, source spoofing,
traceroute, raw sockets, and packet capture are outside 0.3. The provider must
not execute or parse output from an external `ping` command.

## Direct-neighbor lookup

0.3 may query an existing operating-system ARP/NDP entry for a directly
connected address. It exposes neither raw neighbor packets nor mutation of the
neighbor table. Unsupported hosts and absent entries have deterministic typed
results. Phase 0 freezes which native hosts claim this optional operation.

## IPsec

IPsec is transparent deployment policy. BN does not configure or detect keys,
IKE, tunnels, security associations, or kernel policies. Ordinary TCP/UDP
operations may be protected by operating-system IPsec; the capability matrix
records only hosts for which executable deployment evidence exists.

## Ownership and failure

The normative signatures must define ownership after connect, accept, provider
failure, close, handler failure, and interpreter shutdown. All waits, queues,
reads, writes, datagrams, resolver results, and concurrent pings are bounded.
Fallible operations return an explicit success alternative or `Error`; no
network exception bypasses BN control flow.

## Verification boundary

Conformance uses local IPv4/IPv6 services and injected providers. No test
requires public DNS or Internet access. Each host support claim requires
executable evidence for the exact operation claimed.
