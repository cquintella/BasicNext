# Appendix: BNDispatch

BNDispatch is a host-backed external module, imported explicitly with
`IMPORT BNDispatch AS Dispatch`. It supplies bounded queues, tickets, groups,
barriers, semaphores, and mutexes. These APIs are deliberately separate from
the core language and expose no native thread handle.

Queue workers are limited to 64 and pending work to 1,024. Lifecycle waits use
timeouts from 1 to 60,000 milliseconds. Synchronization operations also return
an `Error` on timeout or invalid bounds, so programs can handle resource
pressure explicitly.
