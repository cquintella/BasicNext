# Sprint 3 — HOST.Net neighbor provider

Date: 2026-09-06

## Delivered

- Implemented read-only neighbor lookup in both the shared `bn_rt` provider
  and the interpreter compatibility provider.
- Loopback addresses are valid local neighbors and return the queried address.
- IPv4 lookup reads the Linux ARP table when available and otherwise queries
  the platform `arp` provider with fixed arguments and no shell.
- IPv6 lookup queries the platform `ndp` provider with fixed arguments and no
  shell.
- Only complete entries are accepted; incomplete or mismatched entries become
  `NeighborError::NotFound`.
- The C ABI continues to apply the execution-policy check before lookup.

The language API returns the neighbor IP identity, not the link-layer MAC
address. No table is mutated and no `ping(8)` command is used.

## Verification

```text
cargo test --package bn_rt net::neighbor::tests
passed: 1 parsing test
cargo test --test runtime host_net_ping_loopback_and_neighbor_typed_result
passed: loopback Neighbor returns a valid HOST.Net.Address
cargo clippy --workspace --all-targets -- -D warnings
passed
cargo test
passed: full workspace suite
cargo fmt --check
passed
git diff --check
passed
```

The full workspace test, formatting, lint, and diff gates passed. A missing
ARP/NDP entry is an ordinary `Error` result with
`direct-neighbor entry not found`; it is not an unsupported provider result.
