# TCP and UDP socket example

`socket.bn` is one configurable `HOST.Net` client/server program. It exchanges
`PING` and `PONG` over loopback and the server records every accepted connection
through `BNLog` as JSON Lines.

Run TCP in two terminals:

```sh
bn run examples/socket.bn -- --tcp --server --log tcp-server.log.jsonl
bn run examples/socket.bn -- --tcp --client
```

Run UDP the same way:

```sh
bn run examples/socket.bn -- --udp --server --log udp-server.log.jsonl
bn run examples/socket.bn -- --udp --client
```

Add `--ipv6` to both commands to use `::1`. TCP uses port `39101`, UDP uses
port `39102`, and all network waits are bounded to five seconds. Run `--help`
for the complete option form.
