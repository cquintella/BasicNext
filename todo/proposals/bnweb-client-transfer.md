# Proposal: BNWeb client transfer (curl capabilities)

**Status:** Scheduled as [`bucket-0.6.15-bnweb-transfer-and-quic.md`](../bucket-0.6.15-bnweb-transfer-and-quic.md). This file is still not normative. Where it disagrees with that bucket, or with `bucket-0.6.14b-crypto-pki.md` and `bucket-0.6.15b-bnweb-client-identity.md`, the buckets win.
**Date:** 2026-10-28
**Target:** 0.6.15, after the 0.6.14 MVP (`ToText`, `Hash`, `HashFile`, `ConstantTimeEquals`, `Bytes` wipe) and after 0.6.12 `FS.OpenNew`. It does not wait for 0.6.16 or for 0.6.14b. 0.6.13 is unassigned. It does not enter 0.6.11, it does not replace `todo/bucket-0.6.12-host-filesystem-directories.md`, and it does not absorb `todo/bucket-0.6.14-crypto-enhancements.md`. HTTP/3 is part of 0.6.15. `Trust`, `AddTrust`, `WithClientIdentity`, and `FetchCertificate` are 0.6.15b and start only if CRY-1 reopens X.509.
**Classification:** extension of the BNWeb client surface. No new keyword, no grammar change. The module declaration imports BNCrypto.
**Motivation:** `Client.Request` performs an HTTP exchange and the provider stores the response body, but the Basic Next `Response` value has no body reader. A program cannot download text or bytes through BNWeb. `HOST.Exec` can run `curl` and capture UTF-8 stdout, which is a shell-out, a second policy boundary, and a dead end for binary bodies. The transfer belongs in BNWeb, on the same provider for `bni` and `bnc`.

**Related:** `language/0.6/bnweb.md` (client, URL boundary, errors), `modules/bn/BNWeb.bn`, `language/0.6/bncrypto.md`, `modules/bn/BNCrypto.bn`, `language/0.6/error.md`, `libraries/bn_web`, `libraries/bn_lib_web`, [`bucket-0.6.14-crypto-enhancements.md`](../bucket-0.6.14-crypto-enhancements.md), [`bnweb-auth-0.5.3.md`](bnweb-auth-0.5.3.md), [`host-filesystem-ops-0.5.3.md`](host-filesystem-ops-0.5.3.md).

---

## What already exists

`IMPORT BNWeb AS Web` is required. `HOST` remains the only built-in. The client accepts absolute `http` and `https` URLs, rejects userinfo, drops the fragment, and applies the egress policy after every resolution and every redirect. `EgressPolicy.New(schemes, cidrs, ports, maxRedirects, totalDeadlineMs)` is the policy value. `Client.New(logger)` writes one access record; `NEW Client()` writes none. There is no hidden global logger.

`Client.Request(method, url, body)` and `Client.RequestWithPolicy(...)` return `Response OR Error`. That `Response` is the server writer (`Status`, `SetStatus`, `SetHeader`, `Write`, `Commit`). The provider keeps a body string and never exposes it. `Request.Body(maximum)` is the inbound server body.

The current redirect loop (`libraries/bn_web/src/http.rs`) treats every status 300..399 as a redirect and resends the same method and body. A missing `Location` fails the call. The body ceiling in that path is `bn_limits` `http.max_body_bytes_default` (the 8 MiB message in the client). Allowed client methods in the interpreter are `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, and `HEAD`.

`HTTP_FAILED` (8) is connection, TLS, or protocol failure. An HTTP status, including 404 and 500, is not that failure. A program cannot `NEW Error`.

`BNCrypto.Bytes` exists today (`Length`, `ToHex`, `FromHex`, `FromText`, `Slice`). It is an opaque buffer: key material does not become a `STRING` by itself. The 0.6.14 MVP keeps that class as the data type and adds strict `ToText`, `Hash`, `HashFile`, and `ConstantTimeEquals`. It also adds `PrivateKey`, `PublicKey`, and `Fingerprint`. `Certificate` and `LoadCertificate` are 0.6.14b, after CRY-1. `FetchCertificate` is 0.6.15b. This proposal does not add `Web.Bytes`, a PEM parser, or a hash function.

`BNCompress.Bytes` is a different class (`Length`, `ToHex`, `ToText`). gzip and deflate on `Content-Encoding` are decoded inside the web provider through `bn_core_compress`. The value returned to the program is `Crypto.Bytes`. A raw body is not passed to `BNCompress.Gunzip` by this API. Merging the two buffer classes is outside this proposal.

---

## Design locks (recommended)

These are recommendations for Carlos to lock. They are not decisions.

1. **Capabilities, not flags.** Each curl behaviour that changes the result becomes a named function or a value (`EgressPolicy`, `TLSClientConfig`, `Proxy`, `Headers`, `Query`, `Form`). A `BOOLEAN` argument does not select a behaviour.
2. **HTTP and HTTPS only.** Other curl protocols stay outside BNWeb. See the exclusion table.
3. **Status is data on a read call.** `Get`, `Post`, `Head`, and the other exchange methods return `ClientReply` for 1xx..5xx. Transport, TLS, policy, limits, and timeouts return `Error`. `Download`, `DownloadReplace`, `DownloadResume`, `DownloadDecoded`, and `PutFile` are the `--fail` family: a status outside 200..299 is `Web.HTTP_STATUS`, the body is discarded, and a partial file is removed.
4. **Text and bytes stay apart.** `STRING` is Unicode. The binary body is `Crypto.Bytes`. `BodyText` applies the same UTF-8 rule as `Crypto.Bytes.ToText` and fails with `Web.INVALID_ARGUMENT` when the body is not UTF-8. The code stays a BNWeb code. No method returns a binary body as `STRING`. On release, 0.6.14 wipes `Bytes`. The program takes the text or the hex while it still holds the value. A `Download*` reply does not also keep the body: `BodyText` and `BodyBytes` on that reply are `NOT_FOUND`, and the bytes are the file. Integrity of a file is `Crypto.HashFile`, compared with `Crypto.ConstantTimeEquals`.
5. **The program states the ceiling.** Every exchange takes `maximum AS INTEGER`, the maximum response body in bytes. `maximum < 0` is `INVALID_ARGUMENT`. `maximum` of 0 accepts an empty body and rejects the first body byte with `LIMIT`. The value cannot exceed `http.max_response_body_bytes`. The decompressed size counts against `maximum`. The compressed size counts as well: either size over the ceiling is `LIMIT`.
6. **No hidden configuration.** No `.curlrc`, no `CURL_*` environment, no `HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY` / `ALL_PROXY`, no netrc, no default cookie jar, no URL globbing, no config file. A program that wants a proxy reads `HOST.Env` itself and passes a `Proxy` value.
7. **TLS verifies by default.** The unverified client is `Client.NewUnverified(logger)`, a separate function. It still applies the egress policy. There is no `-k` switch on `Get`.
8. **Defaults that curl leaves implicit are named here.** No retry. No automatic `Referer`. No automatic `Authorization` across hosts. No `Accept-Encoding` except on the `*Decoded` methods. No progress meter. No `Expect: 100-continue` unless the program sets that header. The provider writes `Content-Length` from the body it sends. A program-supplied `Content-Length`, `Host`, `Connection`, `Transfer-Encoding`, `Keep-Alive`, `Upgrade`, or `Proxy-Connection` is `INVALID_ARGUMENT`.
9. **One provider.** `bn_lib_web` and the `bnc` / `bn_rt` adapter call the same `bn_web` client. `bni` does not shell out. `bnc` does not link a second HTTP stack.
10. **Existing calls stay.** `Request`, `RequestWithPolicy`, `EgressPolicy.New`, and server `TLSConfig.FromPEM` keep their signatures and, for the request methods, their current redirect behaviour (resend method and body for 300..399) until a separate decision changes them. New transfers use `ClientReply`.
11. **Crypto owns secrets and certificates. BNWeb owns the HTTP exchange.** `modules/bn/BNWeb.bn` imports BNCrypto, so a program that imports BNWeb loads BNCrypto. BNCrypto does not import BNWeb. A pin is 32 bytes, the SHA-256 of the peer Subject Public Key Info (the BNCrypto KeyId), held in `Crypto.Bytes`. BNWeb compares it in constant time and does not parse hex. A trust anchor is a `Crypto.Certificate`. A client key is a `Crypto.PrivateKey` already decrypted by `LoadPrivateKey`. BNWeb does not take a key password. `AuthorizationBasic` still takes `STRING`s, because the product is a header value, not a `Crypto.Secret`.

### Redirect rule for the new methods

Follow 301, 302, 303, 307, and 308 only, up to `maxRedirects`, re-checking the URL boundary and SSRF on every hop. 304 and 305 are not redirects: 304 is a finished reply; 305 is `EGRESS_DENIED` (a response must not name a proxy). 300 is a finished reply.

On 301, 302, and 303, `POST`, `PUT`, and `PATCH` become `GET` and the body is dropped. 307 and 308 keep the method and the body. `EgressPolicy.PreserveMethod` is the separate factory for the curl `--post301` / `--post302` / `--post303` behaviour.

Cross-host redirects drop `Authorization` and `Proxy-Authorization`. `Cookie` is not copied as a raw header; a jar attached with `UseCookies` selects cookies for the new host. `EgressPolicy.WithCrossHostAuthorization` is the separate factory that forwards `Authorization` (curl `--location-trusted`).

`Request` and `RequestWithPolicy` do not adopt this rule in this proposal.

### Identity

The default `User-Agent` is the fixed string `BasicNext BNWeb`. A `User-Agent` in `Headers` replaces it. The access log keeps today's shape: method, path, protocol, status, body size, `http.duration_ms`, and non-sensitive headers. `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, the query, the body, userinfo, and TLS private material are omitted.

---

## API

Illustrative signatures. Exact text lands in `bnweb.md` only after a lock. Constructors that fail return `T OR Error`. The module declarations become real provider methods; the stub bodies in `modules/bn/BNWeb.bn` are the contract, not the implementation.

### Reply

`BNWeb.bn` gains `IMPORT BNCrypto AS Crypto`. There is no `Web.Bytes` class. `BodyBytes` returns the existing `Crypto.Bytes` (`Length`, `ToHex`, `Slice`, and `ToText` once 0.6.14 Part 4 has landed). One byte is `Slice(body, index, 1)`.

```basic
EXPORT CLASS ClientReply
    PUBLIC FUNCTION Status() AS INTEGER
    PUBLIC FUNCTION HttpVersion() AS STRING
    PUBLIC FUNCTION FinalUrl() AS STRING
    PUBLIC FUNCTION RedirectCount() AS INTEGER
    PUBLIC FUNCTION BodySize() AS INTEGER
    PUBLIC FUNCTION TotalMs() AS INTEGER
    PUBLIC FUNCTION ConnectMs() AS INTEGER
    PUBLIC FUNCTION Header(name AS STRING) AS STRING OR Error
    PUBLIC FUNCTION HeaderCount() AS INTEGER
    PUBLIC FUNCTION HeaderName(index AS INTEGER) AS STRING OR Error
    PUBLIC FUNCTION HeaderValue(index AS INTEGER) AS STRING OR Error
    PUBLIC FUNCTION BodyText(maximum AS INTEGER) AS STRING OR Error
    PUBLIC FUNCTION BodyBytes(maximum AS INTEGER) AS Crypto.Bytes OR Error
    PUBLIC FUNCTION BodyDecodedText(maximum AS INTEGER) AS STRING OR Error
    PUBLIC FUNCTION BodyDecodedBytes(maximum AS INTEGER) AS Crypto.Bytes OR Error
    PUBLIC FUNCTION PeerAddress() AS Net.Address OR Error
END CLASS
```

`Header` is `NOT_FOUND` when the name is absent. Duplicate headers keep receipt order; `Header` returns the first. `BodySize` is the raw size on the wire after transfer-encoding is removed, before content-decoding. `BodyDecoded*` accept `gzip` and `deflate` only. Any other `Content-Encoding`, including `br` and `zstd`, is `UNSUPPORTED_ENCODING`. Identity encoding returns the raw body. A decoded call does not invent `Accept-Encoding`; the `*Decoded` exchange methods do. The provider decodes, then builds one `Crypto.Bytes`. It does not call `BNCompress` from Basic Next.

### Request values

```basic
EXPORT CLASS Headers
    PUBLIC STATIC FUNCTION New() AS Headers OR Error
    PUBLIC FUNCTION Add(name AS STRING, value AS STRING) AS VOID OR Error
    PUBLIC FUNCTION Set(name AS STRING, value AS STRING) AS VOID OR Error
END CLASS

EXPORT CLASS Query
    PUBLIC STATIC FUNCTION New() AS Query OR Error
    PUBLIC FUNCTION Add(name AS STRING, value AS STRING) AS VOID OR Error
    PUBLIC FUNCTION Encode() AS STRING OR Error
END CLASS

EXPORT CLASS Fields
    PUBLIC STATIC FUNCTION New() AS Fields OR Error
    PUBLIC FUNCTION Add(name AS STRING, value AS STRING) AS VOID OR Error
    PUBLIC FUNCTION Encode() AS STRING OR Error
END CLASS

EXPORT CLASS Form
    PUBLIC STATIC FUNCTION New() AS Form OR Error
    PUBLIC FUNCTION AddField(name AS STRING, value AS STRING) AS VOID OR Error
    PUBLIC FUNCTION AddFile(name AS STRING, path AS STRING, contentType AS STRING) AS VOID OR Error
END CLASS
```

`Query.Encode` percent-encodes and uses `%20` for space. `Fields.Encode` is `application/x-www-form-urlencoded` and uses `+` for space. Header and field values reject NUL, CR, and LF. Header names are a bounded HTTP token.

`Form.AddFile` records the path. `PostForm` reads the file during the call, under the remaining body budget. The parent directory is not created. After 0.6.12 the program creates it with `FS.CreateDirectory`. The path is not taken from `Content-Disposition`.

### Policy, TLS, proxy, retry

```basic
EXPORT CONST ADDRESS_ANY AS INTEGER = 0
EXPORT CONST ADDRESS_IPV4 AS INTEGER = 1
EXPORT CONST ADDRESS_IPV6 AS INTEGER = 2

EXPORT CLASS TLSClientConfig
    PUBLIC STATIC FUNCTION SystemTrust() AS TLSClientConfig OR Error
    PUBLIC STATIC FUNCTION Trust(anchor AS Crypto.Certificate) AS TLSClientConfig OR Error
    PUBLIC FUNCTION AddTrust(anchor AS Crypto.Certificate) AS TLSClientConfig OR Error
    PUBLIC STATIC FUNCTION Unverified() AS TLSClientConfig OR Error
    PUBLIC FUNCTION WithClientIdentity(certificate AS Crypto.Certificate, key AS Crypto.PrivateKey) AS TLSClientConfig OR Error
    PUBLIC FUNCTION WithPin(digest AS Crypto.Bytes) AS TLSClientConfig OR Error
    PUBLIC FUNCTION AddPin(digest AS Crypto.Bytes) AS TLSClientConfig OR Error
    PUBLIC STATIC FUNCTION OnlyTls12() AS TLSClientConfig OR Error
END CLASS

EXPORT CLASS Proxy
    PUBLIC STATIC FUNCTION Http(url AS STRING) AS Proxy OR Error
    PUBLIC STATIC FUNCTION HttpBasic(url AS STRING, user AS STRING, password AS STRING) AS Proxy OR Error
    PUBLIC STATIC FUNCTION Https(url AS STRING) AS Proxy OR Error
    PUBLIC STATIC FUNCTION Socks5(url AS STRING) AS Proxy OR Error
    PUBLIC STATIC FUNCTION Socks5Hostname(url AS STRING) AS Proxy OR Error
END CLASS

EXPORT CLASS RetryPolicy
    PUBLIC STATIC FUNCTION None() AS RetryPolicy
    PUBLIC STATIC FUNCTION OnTransport(attempts AS INTEGER, delayMs AS INTEGER) AS RetryPolicy OR Error
END CLASS
```

`ADDRESS_ANY` is happy eyeballs within the connect deadline. `ADDRESS_IPV4` and `ADDRESS_IPV6` are the curl `-4` and `-6` behaviours. An unknown constant is `INVALID_ARGUMENT`.

`SystemTrust` verifies with the host bundle. Raising ordinary HTTPS to a TLS 1.3 floor is still a BDFL gate in 0.6.15. This sentence does not decide it. `OnlyTls12` is the separate factory that names TLS 1.2 and still uses the host bundle. TLS 1.0 and 1.1 are absent. Cipher-suite strings are absent. QUIC is TLS 1.3 because the protocol requires it.

`Trust`, `AddTrust`, `WithClientIdentity`, and `FetchCertificate` are 0.6.15b. They are not part of the 0.6.15 close. The program would load PEM or DER with `Crypto.LoadCertificate` from 0.6.14b. BNWeb does not parse PEM. `WithClientIdentity` would take a `Crypto.Certificate` and a `Crypto.PrivateKey`. The program decrypts a password-protected key with `Crypto.LoadPrivateKey` before that call.

`WithPin` requires a `Crypto.Bytes` of length 32, the SHA-256 of the peer certificate's Subject Public Key Info. `AddPin` adds an alternative; the handshake succeeds when one pin matches. A different length is `INVALID_ARGUMENT`. `Unverified` does not combine with `Trust`, `AddTrust`, `WithPin`, or `AddPin`: those calls are `INVALID_ARGUMENT`.

`FetchCertificate(url AS STRING) AS Crypto.Certificate OR Error` performs a verified handshake under the client's policy and TLS config and returns the peer leaf. It applies the same URL boundary and SSRF checks as `Get`. There is no unverified fetch. The program reads `Fingerprint` or hashes the SPKI and compares with `Crypto.ConstantTimeEquals`. The crypto bucket sketched this as `FetchCertificate(host, port)`; the URL form is recommended so inspection cannot skip the client URL boundary.

`Proxy.Http` is an HTTP proxy URL with no userinfo. Credentials use `HttpBasic`, so the secret is an argument and not part of the URL. `Socks5` resolves the target locally and then applies SSRF. `Socks5Hostname` sends the hostname to the proxy; the proxy URL itself is still resolved and checked. SOCKS4 is deferred.

`RetryPolicy.OnTransport` retries `TIMEOUT` and `HTTP_FAILED` only, with a fixed delay, and only on `Get`, `GetDecoded`, `Head`, `Delete`, `Options`, and the `Download*` methods. `attempts` includes the first try and must be in 1..8. `Post`, `Put`, `Patch`, `PostForm`, `PutFile`, and `RequestMethod` do not retry. The default policy is `None`. There is no retry of `HTTP_STATUS`.

`EgressPolicy` gains factories. The existing `New` stays.

```basic
PUBLIC STATIC FUNCTION WithConnect(schemes AS STRING, cidrs AS STRING, ports AS STRING, maxRedirects AS INTEGER, totalDeadlineMs AS INTEGER, connectMs AS INTEGER) AS EgressPolicy OR Error
PUBLIC STATIC FUNCTION PreserveMethod(schemes AS STRING, cidrs AS STRING, ports AS STRING, maxRedirects AS INTEGER, totalDeadlineMs AS INTEGER) AS EgressPolicy OR Error
PUBLIC STATIC FUNCTION WithCrossHostAuthorization(schemes AS STRING, cidrs AS STRING, ports AS STRING, maxRedirects AS INTEGER, totalDeadlineMs AS INTEGER) AS EgressPolicy OR Error
PUBLIC STATIC FUNCTION WithStall(schemes AS STRING, cidrs AS STRING, ports AS STRING, maxRedirects AS INTEGER, totalDeadlineMs AS INTEGER, minimumBytesPerSecond AS INTEGER, windowMs AS INTEGER) AS EgressPolicy OR Error
```

`WithStall` is curl `--speed-limit` / `--speed-time`. The window is at least one second. A transfer slower than the minimum over that window is `TIMEOUT`.

A bypass list for a proxy is an explicit host list on a further factory once Carlos locks the syntax (`NO_PROXY` is not read). Until that lock, every URL goes through the attached proxy or through none.

### Client

```basic
PUBLIC STATIC FUNCTION NewUnverified(logger AS Log.Logger) AS Client OR Error
PUBLIC FUNCTION UsePolicy(policy AS EgressPolicy) AS VOID OR Error
PUBLIC FUNCTION UseProxy(proxy AS Proxy) AS VOID OR Error
PUBLIC FUNCTION UseCookies(jar AS CookieJar) AS VOID OR Error
PUBLIC FUNCTION UseTLS(config AS TLSClientConfig) AS VOID OR Error
PUBLIC FUNCTION FetchCertificate(url AS STRING) AS Crypto.Certificate OR Error
PUBLIC FUNCTION UseRetry(retry AS RetryPolicy) AS VOID OR Error
PUBLIC FUNCTION UseAddresses(family AS INTEGER) AS VOID OR Error

PUBLIC FUNCTION Get(url AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION GetDecoded(url AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION GetQuery(url AS STRING, query AS Query, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION Head(url AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION Delete(url AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION Options(url AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION Post(url AS STRING, body AS STRING, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PostBytes(url AS STRING, body AS Crypto.Bytes, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PostFields(url AS STRING, fields AS Fields, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PostForm(url AS STRING, form AS Form, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PostJson(url AS STRING, json AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION Put(url AS STRING, body AS STRING, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PutBytes(url AS STRING, body AS Crypto.Bytes, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PutFile(url AS STRING, path AS STRING, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION Patch(url AS STRING, body AS STRING, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION PatchBytes(url AS STRING, body AS Crypto.Bytes, contentType AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION RequestMethod(method AS STRING, url AS STRING, body AS STRING, contentType AS STRING, headers AS Headers, maximum AS INTEGER) AS ClientReply OR Error

PUBLIC FUNCTION Download(url AS STRING, path AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION DownloadReplace(url AS STRING, path AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION DownloadResume(url AS STRING, path AS STRING, maximum AS INTEGER) AS ClientReply OR Error
PUBLIC FUNCTION DownloadDecoded(url AS STRING, path AS STRING, maximum AS INTEGER) AS ClientReply OR Error
```

Each `Get` / `Post` / `Put` / `Patch` / `Delete` / `Head` / `Options` has a sibling `*WithHeaders(..., headers AS Headers, ...)` with the headers argument before `maximum`. The siblings are omitted in the list above to keep it readable; they are part of the claim. A missing header collection means the default `User-Agent` only.

`GetDecoded` and `DownloadDecoded` send `Accept-Encoding: gzip, deflate` when the program did not set `Accept-Encoding`, and they decode before writing or returning the body.

`PostJson` sets `Content-Type` and `Accept` to `application/json`. It does not parse JSON. The program builds the text, including with `BNJson`.

`RequestMethod` accepts an HTTP method token of 1..32 characters. `TRACE` and `CONNECT` are `INVALID_ARGUMENT`. `CONNECT` remains an internal hop of an HTTP proxy, not a user method.

`Head` tolerates a body up to `maximum` and records it. The usual empty body yields `BodyText` of `""`.

### Download and upload

`Download` fails with `ALREADY_EXISTS` when `path` exists. `DownloadReplace` overwrites only after a 2xx response is complete. Neither creates parent directories. The program creates them with `FS.CreateDirectory` from bucket 0.6.12. The remote name is never taken from `Content-Disposition`. `BodyText` and `BodyBytes` on a download reply are `NOT_FOUND`.

`DownloadResume` requires an existing file. It sends `Range: bytes=<size>-`. A 206 appends. A 200 is `HTTP_STATUS` and the existing file is left as it was. A 416 is `HTTP_STATUS`.

On `HTTP_STATUS`, `LIMIT`, `TIMEOUT`, or `HTTP_FAILED` during `Download` or `DownloadReplace`, the provider deletes a partial file it created. `DownloadResume` truncates back to the size at the start of the call. Without `FS.Move`, the write is not atomic across a crash; that limit stays until the filesystem proposal lands.

`PutFile` is curl `-T`: `PUT` of a file. The reply follows the exchange rule (status is data). A non-2xx does not delete the local file.

### Header helpers

Static methods of `Client`:

```basic
PUBLIC STATIC FUNCTION AuthorizationBasic(user AS STRING, password AS STRING) AS STRING OR Error
PUBLIC STATIC FUNCTION AuthorizationBearer(token AS STRING) AS STRING OR Error
```

These return the header value. The program puts it in `Headers`. They are not an OAuth flow; that remains [`bnweb-auth-0.5.3.md`](bnweb-auth-0.5.3.md).

### Cookies

`UseCookies` attaches one jar for the lifetime of the client. The jar stores `Set-Cookie` from replies and sends the matching `Cookie`. `CookieJar` gains:

```basic
PUBLIC FUNCTION Load(path AS STRING) AS VOID OR Error
PUBLIC FUNCTION Save(path AS STRING) AS VOID OR Error
PUBLIC FUNCTION DropSessionCookies() AS VOID
```

`Load` and `Save` use the Netscape cookie-file text, bounded by the jar's existing capacity. A secret in the jar is not logged.

### Parallelism

One `Client` is used from one task. Parallel transfers are several clients, scheduled by the program with `BNDispatch`. BNWeb does not grow a curl multi interface.

---

## Curl map

| Curl | BNWeb |
| --- | --- |
| `curl URL`, `-X GET` | `Get` |
| `-X POST/PUT/PATCH/DELETE`, `-I`, `-X OPTIONS` | `Post`, `Put`, `Patch`, `Delete`, `Head`, `Options` |
| `-X` custom, except `TRACE` and `CONNECT` | `RequestMethod` |
| `-H`, `-A`, `-e` | `Headers.Add` / `Set`. `Referer` is only the header the program sets |
| `-d` / `--data` / `--data-ascii` | `PostFields` |
| `--data-binary`, body that is not Unicode | `PostBytes` / `PutBytes` / `PatchBytes` of `Crypto.Bytes` |
| `--data-urlencode` | `Fields.Add` then `PostFields` |
| `-G`, `--url-query` | `GetQuery` |
| `-F` / `--form` | `Form.AddField`, `Form.AddFile`, `PostForm` |
| `--json` | `PostJson` (no parse) |
| `-o` | `Download` / `DownloadReplace` |
| `-O` | the program chooses the path from `FinalUrl`; no header filename |
| `-C -` | `DownloadResume` |
| `--compressed` | `GetDecoded`, `DownloadDecoded`, `BodyDecodedText`, `BodyDecodedBytes` |
| `--raw` | `BodyBytes` (transfer-encoding already removed by the HTTP stack) |
| `-k` | `Client.NewUnverified` or `TLSClientConfig.Unverified` |
| `--cacert` | `Trust` of a `Crypto.Certificate` from `LoadCertificate` |
| `--cert`, `--key` | `WithClientIdentity` after `LoadCertificate` and `LoadPrivateKey` |
| `--pinnedpubkey` | `WithPin` / `AddPin` of a 32-byte SPKI SHA-256 (`Crypto.FromHex` or `Hash`) |
| `--tlsv1.2` | `OnlyTls12` |
| `--tlsv1.3` | `SystemTrust` (minimum TLS 1.3) |
| peer certificate inspection | `FetchCertificate` |
| `-4`, `-6` | `UseAddresses(ADDRESS_IPV4)` / `ADDRESS_IPV6` |
| `-x`, `--proxy-user`, `--socks5`, `--socks5-hostname` | `Proxy.Http`, `HttpBasic`, `Socks5`, `Socks5Hostname` |
| `-u` | `AuthorizationBasic` |
| `--oauth2-bearer` | `AuthorizationBearer` (the token is the program's) |
| `-b`, `-c`, `--junk-session-cookies` | `UseCookies`, `Load`, `Save`, `DropSessionCookies` |
| `-L`, `--max-redirs`, `--post301/302/303`, `--location-trusted` | redirect rule above; `PreserveMethod`; `WithCrossHostAuthorization` |
| `-m`, `--connect-timeout` | `totalDeadlineMs`, `WithConnect` |
| `--speed-limit`, `--speed-time` | `WithStall` |
| `--retry` on transport errors, idempotent methods | `RetryPolicy.OnTransport` |
| `--fail` | `Download*` and the status rule on `PutFile` only where noted; read calls keep the status |
| `--fail-with-body` | `Get` / `Post` and `BodyText` |
| `-w` | `Status`, `FinalUrl`, `RedirectCount`, `BodySize`, `TotalMs`, `ConnectMs`, `HttpVersion`, `PeerAddress` |
| `-D` / response headers | `Header`, `HeaderCount`, `HeaderName`, `HeaderValue` |
| `-T` | `PutFile` |
| `-Z` / curl multi | `BNDispatch` and one client per task |
| `-s` | the default: no meter |
| `--http1.1`, `--http2` | already required (HTTP/1.1, HTTP/2, ALPN). A forced version waits on an open question |
| existing `Request` | unchanged |

### Deferred, by name

Digest, NTLM, Negotiate, GSS-API, AWS SigV4, OAuth (the dance, not the bearer header), Alt-Svc cache, HSTS persistence, DoH, Unix-domain sockets, SOCKS4, `--resolve` / `--connect-to`, interface bind, cipher-suite strings, `--capath` as a directory, brotli, zstd, remote-name from `Content-Disposition`, `--create-dirs` (the program calls `FS.CreateDirectory`), proxy bypass list syntax, forcing HTTP/1.1 versus HTTP/2, and merging `BNCompress.Bytes` into `Crypto.Bytes`. HTTP/3 left this list on 2026-10-28 and is Part 3 of `todo/bucket-0.6.15-bnweb-transfer-and-quic.md`. Atomic replace via `FS.Move` is in that bucket. Server `TLSConfig.FromPEM` stays unchanged. `TlsServerConfig.FromFiles` is not this proposal.

### Excluded

FTP, FTPS, SFTP, SCP, SMTP, SMTPS, IMAP, POP3, LDAP, MQTT, RTSP, TELNET, DICT, FILE, GOPHER, TFTP, SMB, IPFS, and every other non-HTTP scheme. `TRACE`. User-level `CONNECT`. URL globbing. `--config`, netrc, and environment proxies. Progress meters and `--libcurl` output. Writing response headers into the body (`-i`). Metalink. CRLF mail upload. A silent downgrade from HTTPS to HTTP. Treating a 4xx on `Get` as `Error`.

---

## Errors

New constants, next to the existing 1..9 table. Values are a recommendation.

| Constant | Value | When |
| --- | ---: | --- |
| `Web.HTTP_STATUS` | 10 | `Download`, `DownloadReplace`, `DownloadResume`, `DownloadDecoded`, or a locked fail-closed upload saw a status outside 200..299. `Cause` is the decimal status. The body is not kept. |
| `Web.ALREADY_EXISTS` | 11 | `Download` or `DownloadDecoded` and `path` already exists |
| `Web.UNSUPPORTED_ENCODING` | 12 | `BodyDecoded*` or a `*Decoded` exchange saw a content-encoding other than identity, `gzip`, or `deflate` |

Reuse the current codes otherwise: `INVALID_ARGUMENT` (bad URL, userinfo, method token, header, negative maximum, pin length other than 32, `Unverified` combined with a trust anchor or a pin), `NOT_FOUND` (header, cookie, or a body reader on a `Download*` reply), `LIMIT` (body, header block, redirect count, jar), `TIMEOUT`, `EGRESS_DENIED` (policy, SSRF, 305), `HTTP_FAILED` (connect, TLS, protocol, pin mismatch; `Cause` carries the Net or TLS reason), `UNAVAILABLE` (provider missing in this build). A bad PEM or a bad certificate is a BNCrypto error from `LoadCertificate` or `LoadPrivateKey`, not a BNWeb error.

Messages go in the Fluent catalog. `Operation` is the qualified name (`BNWeb.Client.Download`).

---

## Example

```basic
IMPORT BNWeb AS Web
IMPORT BNCrypto AS Crypto
IMPORT BNLog AS Log

FUNCTION Start() AS VOID
    LET logger AS Log.Logger OR Error = Log.Logger.New("book", "JSON", "CONSOLE")
    IF logger IS Error THEN
        RETURN
    END IF
    LET client AS Web.Client OR Error = Web.Client.New(logger)
    IF client IS Error THEN
        RETURN
    END IF

    LET page AS Web.ClientReply OR Error = client.Get("https://example.com/toc.md", 65536)
    IF page IS Error THEN
        RETURN
    END IF
    IF page.Status() <> 200 THEN
        RETURN
    END IF
    LET text AS STRING OR Error = page.BodyText(65536)
    IF text IS Error THEN
        RETURN
    END IF

    LET saved AS Web.ClientReply OR Error = client.Download("https://example.com/logo.png", "logo.png", 1048576)
    IF saved IS Error THEN
        RETURN
    END IF
    LET digest AS Crypto.Bytes OR Error = Crypto.HashFile("logo.png")
    IF digest IS Error THEN
        RETURN
    END IF
END FUNCTION
```

`HashFile` is 0.6.14 Part 4. The snippet is the shape after that bucket, not a program that runs today.

A 404 on `Get` is a reply whose `Status` is 404. A 404 on `Download` is `Error` 10 and leaves no file.

---

## Acceptance

No implementation is claimed by this file. After a lock, the bucket needs, together:

- `language/0.6/bnweb.md` and `modules/bn/BNWeb.bn` updated before provider code. The module imports BNCrypto.
- The three new codes in `crates/bn_types/src/error_codes.rs` and the catalog.
- The 0.6.14 MVP rows this close needs (`ToText`, `Hash`, `HashFile`, `ConstantTimeEquals`) are in the spec before provider code. Certificate fixtures are 0.6.15b, after 0.6.14b `LoadCertificate`.
- Fixtures on `bni` and `bnc`: text `Get` round-trip; `BodyBytes` is `Crypto.Bytes` and `Crypto.Hash` of it matches a known digest; binary `Download` round-trip checked with `Crypto.HashFile`; `BodyBytes` on that reply is `NOT_FOUND`; 404 `Get` is a reply; 404 `Download` is `HTTP_STATUS` and creates no file; userinfo and a loopback redirect are `EGRESS_DENIED`; `BodyText` of invalid UTF-8 is `Web.INVALID_ARGUMENT`; gzip `GetDecoded` charges the decompressed size and does not call public `BNCompress.Inflate` or `Gunzip`; `TRACE` is rejected; `NewUnverified` is a different function and still denies a blocked CIDR; a 32-byte pin mismatch is `HTTP_FAILED`; a pin of any other length is `INVALID_ARGUMENT`. `Trust` and `WithClientIdentity` fixtures are WEB-8b in 0.6.15b.
- The old `Request` fixture stays on the old redirect behaviour.
- Book and man text only after the contract exists. The book packer is the motivating program; updating it is a separate change.

---

## Open questions

1. `ClientReply` as its own type, or body readers on the existing `Response`? Recommendation: `ClientReply`, so `Request` still returns `Response` and the server writer stays a writer.
2. Confirm the 301/302/303 method change for the new methods, and whether `Request` ever adopts it.
3. Codes 10, 11, and 12.
4. `Download` fails when the file exists. `DownloadReplace` is the overwrite.
5. Proxy bypass syntax.
6. Forced HTTP version, or ALPN as today.
7. TLS 1.3 for every ordinary HTTPS connection. Still a BDFL gate in 0.6.15. This file does not set that floor. QUIC stays TLS 1.3.
8. Closed 2026-10-28: 0.6.15 ships the transfer and QUIC on the 0.6.14 MVP. `Trust`, `WithClientIdentity`, and `FetchCertificate` are 0.6.15b and wait for 0.6.14b `LoadCertificate` and for CRY-1.

`WithPin` is in the claim. The buffer type is `Crypto.Bytes`. Those two points are no longer open.

## History

- **2026-10-28** — Proposed from the client body gap. `Request` stores a body and returns a writer with no reader. Curl is the capability list, not the API shape.
- **2026-10-28** — Revised for bucket 0.6.14. Dropped `Web.Bytes`. Bodies, pins, and checksums use BNCrypto. Certificates and private keys come from Part 6. TLS configuration stays in BNWeb, as Part 7 of that bucket already required.
- **2026-10-28** — Scheduled as `todo/bucket-0.6.15-bnweb-transfer-and-quic.md`. HTTP/3 is in that bucket (`UseHttp3`, `UseHttp3Only`, `StartHttp3`). The deferral list below is the proposal's original cut; the bucket is the plan.
- **2026-10-28** — Split. 0.6.15 does not wait for certificates. Identity APIs moved to `todo/bucket-0.6.15b-bnweb-client-identity.md`. X.509 moved to `todo/bucket-0.6.14b-crypto-pki.md` and stays behind CRY-1.
