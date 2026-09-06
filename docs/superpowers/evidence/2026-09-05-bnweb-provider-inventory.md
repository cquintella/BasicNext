# BNWeb provider inventory — 0.4.4 Sprint 2

This is the frozen discovery inventory for SECTION 2, ACTIVITY 2.2. Rows are
limited to methods advertised by `modules/bn/BNWeb.bn`; a passing unit test is
evidence only for the listed method and target.

| Surface | Interpret/native status | Evidence | Classification |
| --- | --- | --- | --- |
| Request, Response, HeaderValues, QueryValues | implemented | `cargo test --lib web::`, 45 passing tests; `src/runtime/executor/part4.rs` | implemented with evidence |
| Server, ServerOptions | implemented | `src/http/tests.rs`, `src/web/tests.rs` lifecycle, quota, routing tests | implemented with evidence |
| EgressPolicy | implemented | `src/http/tests.rs` policy/redirect tests; `src/web/tests.rs` fail-closed tests | implemented with evidence |
| CookieJar | implemented | `src/web_state.rs` unit tests; runtime dispatch in `part17.rs` | implemented with evidence |
| SessionStore | implemented | `src/web_state.rs` unit tests; runtime dispatch in `part17.rs` | implemented with evidence |
| Scraper | implemented | `src/web_state.rs` parser/bounds tests; runtime dispatch in `part17.rs` | implemented with evidence |
| ACL | implemented | runtime dispatch and address checks in `part17.rs` | implemented with evidence |
| TLSConfig / StartTLS | implemented | `src/tls.rs` PEM/provider tests; `part4.rs` TLS listener dispatch; `http.rs` TLS serving tests | implemented with evidence |
| Client.Request HTTPS | implemented for hosts with a system CA bundle | `src/http.rs::perform_http_request`, `src/tls.rs::client_config`, HTTPS policy test | implemented with evidence; certificate-store availability is an explicit host prerequisite |

The inventory is intentionally not a blanket “provider available” claim. The
two TLS rows require implementation or an explicit contract decision before G2.
