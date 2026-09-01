# Parallel examples

These examples demonstrate the bounded `BNDispatch` provider without network
I/O:

```shell
bn check examples/parallel_work.bn
bn run examples/parallel_work.bn
bn check examples/parallel_pi.bn
bn run examples/parallel_pi.bn
```

`parallel_work.bn` submits four independent range reductions to
`Queue.Concurrent(4)`. `parallel_pi.bn` divides the Leibniz series into four
independent quarter-series workers. Each worker prints its own contribution;
the current 0.3 ticket API does not expose a shared mutable result or a return
value, so the examples deliberately demonstrate bounded submission, concurrent
execution, output forwarding, `Join`, and `Close` without inventing an
aggregation API. Worker output order is nondeterministic.
