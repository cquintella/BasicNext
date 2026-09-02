# Parallel examples

These examples demonstrate the bounded `BNDispatch` provider without network
I/O:

```shell
bn check examples/parallel_work.bn
bn run examples/parallel_work.bn
bn check examples/parallel_pi.bn
bn run examples/parallel_pi.bn
bn check examples/dispatch_game_tournament.bn
bn run examples/dispatch_game_tournament.bn
bn check examples/dispatch_reliability_simulation.bn
bn run examples/dispatch_reliability_simulation.bn
bn check examples/dispatch_cellular_automaton.bn
bn run examples/dispatch_cellular_automaton.bn
```

`parallel_work.bn` submits four independent range reductions to
`Queue.Concurrent(4)`. `parallel_pi.bn` divides the Leibniz series into four
independent quarter-series workers. Each worker prints its own contribution;
the current 0.3 ticket API does not expose a shared mutable result or a return
value, so the examples deliberately demonstrate bounded submission, concurrent
execution, output forwarding, `Join`, and `Close` without inventing an
aggregation API. Worker output order is nondeterministic.

## Recovery-validation examples

The following programs use useful, deterministic workloads and wait for each
ticket in a declared order. They validate task submission, isolated local
computation, bounded ticket output, `AWAIT`, and successful close without
treating operating-system scheduling order as an assertion.

- `dispatch_game_tournament.bn` evaluates 1,000 rounds of each Prisoner's
  Dilemma strategy pairing.
- `dispatch_reliability_simulation.bn` simulates periodic outages and costs for
  four independent production lines over 120 days.
- `dispatch_cellular_automaton.bn` evolves four elementary cellular-automaton
  rules on separate eight-cell rings and prints final checksums.

The runtime suite executes all three examples with exact expected output.
They are acceptance evidence for the task/queue surface, not substitutes for
the provider-internal tests required by the reopened BNDispatch recovery gate.
`Barrier`, `Semaphore`, `Mutex`, and `Group` require Rust-level concurrency
tests until the public async surface can pass synchronization handles safely to
named task functions.
