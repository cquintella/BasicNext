# Sprint 4 audit

Status: Unix complete. Remaining Windows real-TTY capture is **R19**, not a
Sprint 4 language reopen.

Normative sources reviewed: `docs/language/0.2/0.2.ebnf`,
`docs/language/0.2/0.2.md`, `docs/language/0.2/keywords.md`,
`docs/library/console.md`, `ongoing/WBS-0.2.md`, `ongoing/bucket.md`.

| Requirement | Evidence | Result |
| --- | --- | --- |
| Withdraw uppercase console statements | `tests/grammar/invalid/withdrawn-console-statements.bn`; semantic fixture sweep | pass |
| `Cls`/`Beep` stream-safe methods | `tests/grammar/valid/cls-and-beep.bn`; runtime test `executes_cls_and_beep_through_host_console` | pass |
| Console method signatures and direct/alias access | semantic host member table; valid fixture | pass |
| TTY-only calls fail at execution, not analysis | `console_tty_calls_fail_only_when_executed` | pass |
| PrintAt coordinate/string bounds and ANSI behavior | runtime host implementation | pass |
| Current window dimensions | Unix stdout `ioctl` at the call; Win32 console API | macOS/Linux pass; Windows is R19 |

Direct CLI evidence:
- `cargo run --quiet -- check tests/grammar/valid/cls-and-beep.bn`: pass
- `cargo run --quiet -- run tests/grammar/valid/cls-and-beep.bn`: pass
- macOS 26.5.2 PTY: `tests/grammar/valid/console-size.bn` returned `80, 24`.
  After `stty cols 101 rows 37`, it returned `101, 37`.
- Linux (Apple Container, Rust 1.97): after `stty cols 101 rows 37`, the same
  fixture returned `10137` and exited with code 0.

Open requirements: R19 Windows real-TTY traces.
