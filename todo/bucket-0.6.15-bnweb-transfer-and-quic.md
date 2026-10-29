# Bucket 0.6.15 — BNWeb Client Transfer and QUIC

**Status:** todo, created 2026-10-28. Starts after 0.6.12 (`FS.OpenNew`, `FS.Move`, `FS.CreateDirectory`) and the 0.6.14 MVP (`ToText`, `Hash`, `HashFile`, `ConstantTimeEquals`, `Bytes` wipe). It does not wait for 0.6.16 or for 0.6.14b. 0.6.13 is unassigned. `Trust`, `WithClientIdentity`, and `FetchCertificate` are `todo/bucket-0.6.15b-bnweb-client-identity.md`.
**Source:** `todo/proposals/bnweb-client-transfer.md`. That file stays non-normative. This bucket is the work plan. `language/0.6/bnweb.md` and `modules/bn/BNWeb.bn` change before provider code.
**Gate:** Quorra before Carlos. Every row is an extension.

The proposal's recommended locks are the design of this bucket: capabilities are functions and values, not flags; `Request` / `RequestWithPolicy` / server `TLSConfig.FromPEM` keep their signatures and the current redirect loop; new exchanges return `ClientReply`; binary bodies are `Crypto.Bytes`; HTTP status on a read is data; `Download*` outside 200..299 is `Web.HTTP_STATUS`.

## Consolidated duplicates

| Alias | Where | Disposition |
| --- | --- | --- |
| Client transfer proposal | `todo/proposals/bnweb-client-transfer.md` | Source of Parts 1–4. Not normative. |
| "HTTP/3 remains deferred" | `language/0.6/bnweb.md` protocol scope and state section; `language/0.6/0.6.md` BNWeb paragraph and "Still outside 0.5" | Closed only when **QUIC-1** lands in the spec. Until then the sentences stay true. |
| TLS helpers parked in BNCrypto | `todo/bucket-0.6.14-crypto-enhancements.md` (boundary note) and `todo/bucket-0.6.15b-bnweb-client-identity.md` | This close uses `Bytes`, `Hash`, `HashFile`, `ToText`, and `ConstantTimeEquals`. `Trust`, `WithClientIdentity`, and `FetchCertificate` are 0.6.15b. They are not implemented here. |
| `Web.Bytes` | dropped in the proposal revision of 2026-10-28 | Do not reintroduce. |
| Socket quota counted twice, and not by BNWeb | `bn_host_net` handle maps and `bn_rt/src/net_abi/handles.rs`; `bn_web` calls `TcpStream::connect` and `into_std` without either table | **NET-1** through **NET-4**. One counter in `bn_core_net`. |

## BDFL gates

These rows have no activity until Carlos records the choice. Do not invent a syntax for them inside an implementation task.

1. **Proxy bypass list.** No `NO_PROXY` read. Until a factory is locked, a client with a `Proxy` sends every URL through it, and a client without a `Proxy` sends none.
2. **TLS 1.2 on ordinary HTTPS.** This bucket does not raise the TCP client floor to TLS 1.3. QUIC is TLS 1.3 because the protocol requires it. Adopting "all HTTPS is TLS 1.3 unless `OnlyTls12`" is a separate decision.
3. **0-RTT.** Early data is outside this bucket.

## Part 0: One socket core

`0.6.md` already says the native provider applies the socket-handle quota to TCP streams, TCP listeners, and UDP sockets. The code does not. `bn_host_net` counts its own maps. `bn_rt/src/net_abi/handles.rs` counts another table. `libraries/bn_web` connects and accepts with `bn_core_net::TcpStream` / `TcpListener`, then `into_std()` hands the fd to tokio and hyper. That socket is in neither table. UDP exists only as `bn_core_net::UdpSocket` for `HOST.Net`. `bn_web` never binds one. A QUIC engine that called `std::net::UdpSocket::bind` would be a third path.

Every TCP and UDP fd this process opens for Basic Next is born in `bn_core_net`. That includes `HOST.Net`, the BNWeb client, the BNWeb server, and HTTP/3. The HTTP bytes stay in hyper. The QUIC packets stay in the QUIC engine. Neither stack calls `tcp_read`, `tcp_write`, `udp_send_to`, or `udp_receive` for those transfers. The program-facing `HOST.Net` methods keep their signatures.

The quota is one counter inside `bn_core_net`, acquired when a stream connects, a listener binds, a connection is accepted, or a UDP socket binds, and released when the last owner drops. The interpreter maps and the C ABI table become indexes. They stop counting. `into_std` today drops the wrapper and would release the quota while hyper or the QUIC engine still holds the fd. It returns the standard socket together with a permit, and the caller holds that permit until the fd is closed. `BNWeb` maps a failed acquire to `Web.LIMIT`. `HOST.Net` keeps `Net.LIMIT`.

`UdpSocket::send_to` still refuses multicast and broadcast. That check belongs to the program API. The QUIC engine does not send through `SendTo`. It receives a permitted socket from `udp_bind` and may drive it. Before the engine may send to a new peer address, BNWeb runs the same egress check it uses for TCP. A QUIC packet is well under `datagram_max_bytes` (65 536). A configuration that puts that ceiling under 1 200 bytes makes `UseHttp3`, `UseHttp3Only`, and `StartHttp3` return `Web.LIMIT`.

## Out of this bucket

Digest, NTLM, Negotiate, GSS-API, AWS SigV4, the OAuth dance, a JWKS HTTP client, Alt-Svc cache, HSTS persistence, DoH, Unix sockets, SOCKS4, `--resolve` / `--connect-to`, interface bind, cipher-suite strings, `--capath` as a directory, brotli, zstd, `Content-Disposition` filenames, parent-directory creation, MASQUE, HTTP/3-to-proxy, server push, WebTransport, QUIC datagrams as a program API, connection migration onto an address the egress policy has not allowed, and merging `BNCompress.Bytes` into `Crypto.Bytes`. FTP and the other non-HTTP curl schemes stay out. `TRACE` and user `CONNECT` stay rejected.

---

## Part 1: Reply, verbs, and bodies

`modules/bn/BNWeb.bn` imports BNCrypto. BNCrypto does not import BNWeb. Importing BNWeb loads BNCrypto. One HTTP provider in `libraries/bn_web` serves `bni` (`bn_lib_web`) and `bnc`. QUIC and HTTP crates stay in that library. `crates/bn_rt` and `crates/bnc` do not gain a dependency on them. The fds those crates use come from `bn_core_net` (Part 0).

`ClientReply` is the result of the new methods. `HttpVersion` is `HTTP/1.1`, `HTTP/2`, or `HTTP/3`. `BodyBytes` / `BodyDecodedBytes` return `Crypto.Bytes`. `BodyText` / `BodyDecodedText` use the UTF-8 rule of `Crypto.Bytes.ToText` and return `Web.INVALID_ARGUMENT` on a failure. `Download*` replies answer `NOT_FOUND` from the body readers; the bytes are the file. The program checks a file with `Crypto.HashFile` and `Crypto.ConstantTimeEquals`.

Verbs, header siblings, `Query`, `Fields`, `Form`, `PostJson`, `RequestMethod`, and the header helpers `AuthorizationBasic` / `AuthorizationBearer` are the signatures in the proposal. `maximum` is required, rejects a negative value, and cannot exceed `http.max_response_body_bytes`. Both the compressed and the decompressed sizes count. gzip and deflate are decoded inside the provider through a bounded entry on `bn_core_compress`. The public `Inflate` and `Gunzip` stay capped at 256 MiB and are not called here: they have no caller limit. The bounded entry takes `maximum` and never allocates above it, and never above 256 MiB. The program never receives `BNCompress.Bytes` from BNWeb.

Redirects for the new methods follow 301, 302, 303, 307, and 308 only, re-check the URL boundary and SSRF, and turn `POST` / `PUT` / `PATCH` into `GET` on 301, 302, and 303. `EgressPolicy.PreserveMethod` and `EgressPolicy.WithCrossHostAuthorization` are the named alternatives. 305 is `EGRESS_DENIED`. The old `Request` loop is unchanged.

Default `User-Agent` is `BasicNext BNWeb`. Hop-by-hop headers and a program-supplied `Content-Length` or `Host` are `INVALID_ARGUMENT`. The access log of a new exchange records the actual protocol. `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, the query, the body, userinfo, and TLS private material stay out of the log.

## Part 2: Download, upload, policy, TLS

`Download` fails with `ALREADY_EXISTS` when the path exists. `DownloadReplace` writes a provider-owned temporary file in the same directory and `FS.Move`s it onto the path only after a complete 2xx. A failed move deletes the temporary file. `DownloadResume` sends `Range` from the current size; 206 appends; 200 and 416 are `HTTP_STATUS` and leave the file at its previous size. `PutFile` is `PUT` of a file and keeps a non-2xx status as data.

`Download`, `PutFile`, `Form.AddFile`, and cookie `Load` / `Save` go through `HOST.FileSystem` and its sandbox. The provider does not open those paths with `std::fs` on its own. The temporary file and the cookie file are created with `FS.OpenNew`, not with `FS.Open` `WRITE`, not with `FS.Copy`, and not with a chmod afterwards. A destination that is a symlink fails closed. `DownloadReplace` creates that private temporary and then `FS.Move`s it onto the destination. Every failure unlinks a temporary file this call created. gzip and deflate allocate no output buffer larger than `maximum`.

`Error.Message`, `Error.Cause`, and the access log do not contain a password, a bearer token, a cookie, a PEM, or a pin. The access record of a new exchange includes `tls.verified`. Retries share `totalDeadlineMs` and stop if any response byte has arrived. The old `Request` loop, which resends the method and body on every 3xx, stays as specified and is not the pattern for the new methods.

`UsePolicy`, `UseProxy`, `UseCookies`, `UseTLS`, `UseRetry`, and `UseAddresses` configure the client. `RetryPolicy.OnTransport` retries `TIMEOUT` and `HTTP_FAILED` only, on the idempotent methods, at most 8 attempts. `Proxy` has `Http`, `HttpBasic`, `Https`, `Socks5`, and `Socks5Hostname`. No environment proxy and no userinfo in the proxy URL.

`TLSClientConfig.SystemTrust` uses the host bundle and the current TCP TLS floor. `OnlyTls12` is the named TCP configuration that stops at TLS 1.2. `WithPin` / `AddPin` take a 32-byte `Crypto.Bytes`, the SHA-256 of the peer Subject Public Key Info, compared in constant time. `NewUnverified` does not combine with trust anchors or pins. `Trust`, `AddTrust`, `WithClientIdentity`, and `FetchCertificate` are 0.6.15b and are absent from this close. Cookie `Load` / `Save` use the bounded Netscape file. `DropSessionCookies` is its own method.

New codes, beside 1..9: `HTTP_STATUS` 10, `ALREADY_EXISTS` 11, `UNSUPPORTED_ENCODING` 12. Messages live in the Fluent catalog.

## Part 3: QUIC and HTTP/3

HTTP/3 is not a boolean on `Get` and not a new scheme. The URL stays `https`. An `http` URL on a QUIC client is `INVALID_ARGUMENT`.

```basic
PUBLIC FUNCTION UseHttp3() AS VOID OR Error
PUBLIC FUNCTION UseHttp3Only() AS VOID OR Error
PUBLIC FUNCTION StartHttp3(endpoint AS Net.Endpoint, config AS TLSConfig) AS VOID OR Error
```

`UseHttp3` sends QUIC only to an address that already passed `validate_ssrf_destinations`. It falls back to the TCP client only when that address answers with a QUIC version negotiation or a transport error showing it does not speak HTTP/3. Silence, timeout, ICMP unreachable, and a reset do not fall back: those are `HTTP_FAILED`. An on-path attacker who can forge that negotiation can still force TCP. `HttpVersion` is the protocol that carried the response. A missing QUIC stack in the build is `UNAVAILABLE` at the `Use*` call, with no TCP disguise. The engine does not resolve the name a second time.

`UseHttp3Only` completes the exchange on HTTP/3 or returns `HTTP_FAILED`. It does not open TCP. `OnlyTls12` together with either `Use*` call is `INVALID_ARGUMENT`. `NewUnverified` still applies: the QUIC handshake encrypts, and certificate verification stays off only on that client.

`StartHttp3` binds a `bn_core_net::UdpSocket` and serves the existing route, filter, session, and log pipeline. `Start` and `StartTLS` stay on `bn_core_net::TcpListener`. ALPN is `h3`. The server sends `ENABLE_PUSH` 0 and does not deliver a push to user code. QPACK output is subject to the existing header limits. The body ceiling is the same `maximum` and the same `bn_limits` cap. The engine does not call `std::net::UdpSocket::bind` or `tokio::net::UdpSocket::bind`.

Egress is checked on the address that will be used, including a QUIC `preferred_address` and any migrated peer address, after IPv4-mapped IPv6 is folded to IPv4. The check is `validate_ssrf_destinations`, not a second list. Application data moves to a new path only after QUIC path validation and that check. An address the policy denies is `EGRESS_DENIED` and is not contacted. A configured `Proxy` does not carry QUIC in this bucket. `UseHttp3Only` with a proxy is `HTTP_FAILED`. `UseHttp3` with a proxy uses the TCP client.

`Socks5` resolves locally and then applies the same check. `Socks5Hostname` sends the name to the proxy, so a CIDR allowlist cannot be enforced. It is `EGRESS_DENIED` unless the policy's CIDR list allows every address. The URL scheme and authority rules still apply before the name is sent.

A `StartHttp3` socket obeys the QUIC amplification limit: until the client address is validated, the server sends at most three times the bytes it has received from that address. Retry tokens come from the existing unseeded `getrandom` path in BNCrypto (bucket 0.6.14, RNG-1), not from a counter and not from a seeded generator. The same bound caps a migration probe.

`BodyBytes` is `Crypto.Bytes`. BNCrypto wipes that buffer on the last `RELEASE` and does not `mlock` it: a response body is not a `SecretKey`. This bucket does not fetch a JWKS. A program that needs a remote set calls `Get` and passes the text to `Crypto.LoadJwks`.

Name resolution stays `bn_core_net::resolve`. Idle and handshake waits stay inside `totalDeadlineMs` and `tls_handshake_ms`. No new unbounded timer.

---

## Activity checklist

### Part 0: Shared sockets

- [ ] **NET-1 (Quota in the core):** Move the process socket counter into `bn_core_net`. `TcpStream::connect`, `TcpListener::bind` / `bind_with_backlog`, accept, and `UdpSocket::bind` acquire it. Drop of the last owner releases it. `bn_host_net` and `bn_rt/src/net_abi/handles.rs` keep their indexes and stop applying `socket_handles_max` themselves. One fd counts once. A `TCPListen` of N endpoints counts N. Both backends, one process.
- [ ] **NET-2 (Permit across `into_std`):** `into_std` returns the standard socket and a permit. The existing call sites in `libraries/bn_web/src/http.rs` and the server accept loop in `bn_lib_web` hold the permit for the life of the tokio stream. A dropped wrapper must not release the quota while hyper still owns the fd. The same shape exists for `UdpSocket`, and only Part 3 consumes it.
- [ ] **NET-3 (BNWeb uses that path):** The client connects with `bn_core_net::TcpStream::connect`. The TCP server binds and accepts with `bn_core_net::TcpListener`. A failed acquire is `Web.LIMIT` on the BNWeb operation and `Net.LIMIT` on the `HOST.Net` operation. Server admission (`activeConnections`) still applies on top of the process quota. No new `HOST.Net` method and no change to `udp_send_to` / `udp_receive`.
- [ ] **NET-4 (Fixtures):** Fill the quota with `HOST.Net` sockets and show the next `BNWeb` listen or client connect returns `Web.LIMIT`. Fill it with a BNWeb listener and show the next `HOST.Net.TCPConnect` or `UDPBind` returns `Net.LIMIT`. Closing the owner frees the slot. `bni` and `bnc`.

### Part 1: Contract and TCP transfer

- [ ] **WEB-1 (Spec):** Extend `language/0.6/bnweb.md` and `modules/bn/BNWeb.bn` with `ClientReply`, the verbs and `*WithHeaders` siblings, `Headers`, `Query`, `Fields`, `Form`, the download and upload methods, the policy factories, `TLSClientConfig` (`SystemTrust`, `OnlyTls12`, pins, `NewUnverified`), `Proxy`, `RetryPolicy`, the cookie file methods, and codes 10..12. Record the redirect rule and the unchanged `Request` loop. Add the `IMPORT BNCrypto` dependency. Do not add `Trust`, `WithClientIdentity`, or `FetchCertificate`. No provider change in this activity.
- [ ] **WEB-2 (Catalog and codes):** Register `HTTP_STATUS`, `ALREADY_EXISTS`, and `UNSUPPORTED_ENCODING` in `crates/bn_types/src/error_codes.rs` and the Fluent catalog. `Operation` names are qualified (`BNWeb.Client.Download`).
- [ ] **WEB-3 (Reply and verbs):** Implement the read methods on the shared `bn_web` client after **NET-3**. `BodyBytes` returns `Crypto.Bytes`. The connect is the permitted `TcpStream`. Wire `bn_lib_web` and the compiled BNWeb binding to that client. Both backends.
- [ ] **WEB-4 (Decode and forms):** gzip and deflate through a new bounded `bn_core_compress` entry whose limit is `maximum`. Do not call `inflate` or `gunzip`. `PostFields`, `PostForm`, `PostJson`, `PostBytes`, `PutBytes`, `PatchBytes`, `PutFile`.
- [ ] **WEB-5 (Download):** `Download`, `DownloadReplace` via `FS.Move`, `DownloadResume`, `DownloadDecoded`. Partials removed on `HTTP_STATUS`, `LIMIT`, `TIMEOUT`, and `HTTP_FAILED`. Body readers on these replies return `NOT_FOUND`.

### Part 2: Policy and TLS material

- [ ] **WEB-6 (Policy):** `UsePolicy`, connect timeout, stall window, `PreserveMethod`, `WithCrossHostAuthorization`, `UseAddresses`, `UseRetry`, `UseProxy`, `UseCookies`, Netscape `Load` / `Save`, `DropSessionCookies`.
- [ ] **WEB-7 (TLS):** `SystemTrust`, `OnlyTls12`, `WithPin`, `AddPin`, `NewUnverified`. Pins are 32-byte SPKI SHA-256, compared in constant time. No `Certificate` parameter in this bucket.

### Part 3: QUIC

- [ ] **QUIC-1 (Spec):** Remove the "HTTP/3 deferred" sentences from `bnweb.md` and the two `0.6.md` sites listed above. State `UseHttp3`, `UseHttp3Only`, `StartHttp3`, the `http` rejection, the push refusal, the preferred-address check, and the proxy limitation. Version table entry in `0.6.md` when Carlos accepts the text.
- [ ] **QUIC-2 (Stack):** Add one QUIC and HTTP/3 implementation inside `libraries/bn_web`, on the rustls already linked there. No second TLS stack. No QUIC crate in `crates/bn_rt` or `crates/bnc`. The engine's only UDP socket is the permitted `bn_core_net::UdpSocket` from **NET-2**. It does not bind a socket of its own. Record the crate names and versions in this bucket when the activity starts. A build without the stack returns `UNAVAILABLE` from `UseHttp3`, `UseHttp3Only`, and `StartHttp3`. A `datagram_max_bytes` under 1 200 returns `Web.LIMIT` from those three calls.
- [ ] **QUIC-3 (Client):** `UseHttp3` fallback and `UseHttp3Only` failure, TLS 1.3, ALPN `h3`, the same verbs and the same body ceiling. `HttpVersion` is `HTTP/3` only when the response arrived on HTTP/3.
- [ ] **QUIC-4 (Server):** `StartHttp3` binds through `UdpSocket::bind` and holds the permit. Existing `TLSConfig`, routes, filters, and access log. Protocol field `HTTP/3`. Push disabled.
- [ ] **QUIC-5 (Egress):** Resolution, redirect, `preferred_address`, and migration re-check the egress policy. A denied address is `EGRESS_DENIED`.

### Part 4: Security locks

- [ ] **SEC-1 (No silent downgrade):** Implement the `UseHttp3` fallback rule above. A test drops UDP and expects `HTTP_FAILED`, not a TCP response. A test peer that answers with a QUIC version negotiation falls back and reports `HTTP/1.1` or `HTTP/2`.
- [ ] **SEC-2 (Address binding):** The TCP connect and the QUIC engine use the `IpAddr` that passed `validate_ssrf_destinations`. A resolver double that changes its answer between the check and the connect is not consulted again. `Socks5Hostname` with a restrictive CIDR list is `EGRESS_DENIED`.
- [ ] **SEC-3 (QUIC server):** Amplification stays within three times the bytes received until the address is validated. A spoofed Initial does not draw a larger flight. Retry tokens come from the unseeded BNCrypto CSPRNG. Migration does not carry application data before path validation.
- [ ] **SEC-6 (Crypto boundary):** `BodyBytes` is wiped on release and is not `mlock`ed. No JWKS client is added here or inside BNCrypto.
- [ ] **SEC-4 (Files and secrets):** Temp and cookie files use `FS.OpenNew` and do not follow symlinks. `FS.Open` `WRITE` is not that create. Reads and writes of program paths use `HOST.FileSystem`. A sandbox escape is `POLICY_DENIED` from that capability, surfaced as `Web` `Cause`. The bounded inflate stops at `maximum`. A redirect to another host drops `Authorization` unless `WithCrossHostAuthorization` was selected. `Cause` of a failed `HttpBasic` proxy does not contain the password.
- [ ] **SEC-5 (Residual, no code):** `Request` / `RequestWithPolicy` keep the old redirect loop. `NewUnverified` still disables certificate checks; the log shows `tls.verified` false. `WithCrossHostAuthorization` and `PreserveMethod` forward material because the program asked. 0-RTT stays out.

### Part 5: Evidence and teaching

- [ ] **WEB-8 (Fixtures):** `bni` and `bnc`, no public network. Text `Get`; `Crypto.Hash` of `BodyBytes`; `Download` round-trip via `HashFile`; 404 `Get` is a reply; 404 `Download` is code 10 and creates no file; userinfo and a loopback redirect are `EGRESS_DENIED`; invalid UTF-8 `BodyText` is code 1; gzip charges the decompressed size; `TRACE` is rejected; `NewUnverified` still denies a blocked CIDR; a pin of the wrong length is code 1; a pin mismatch is code 8. The `Trust` fixture is 0.6.15b.
- [ ] **QUIC-6 (Fixtures):** In-process HTTP/3 server. `UseHttp3Only` `Get` returns `HTTP/3` and the body. The same server with a plain `Get` stays on TCP. `UseHttp3Only` against a TCP-only listener is `HTTP_FAILED`. An `http` URL is `INVALID_ARGUMENT`. A `preferred_address` in a denied CIDR is `EGRESS_DENIED` and is not contacted. `UseHttp3` with a `Proxy` stays on TCP. The HTTP/3 listener occupies one slot of the **NET-4** quota. **SEC-1** and **SEC-3** are part of this fixture. Linux, macOS, and Windows at the cluster boundary.
- [ ] **WEB-9 (Book and man):** Appendix and man page only after the fixtures pass. The book teaches `ClientReply` and one HTTP/3 example. It does not teach the excluded protocols.
