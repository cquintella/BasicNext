# Using Basic Next 0.1

> Historical 0.1 usage snapshot. For the current release, use
> [`docs/project/usage.md`](../../docs/project/usage.md).

The reference tool is `bn`. It checks and runs `.bn` programs through typed
BN IR. There is no `bn build` in 0.1.

## Install

From this repository:

```shell
cargo install --path .
```

Or download a prebuilt binary from
[GitHub Releases](https://github.com/cquintella/BasicNext/releases/latest).
Asset names are listed in [`binaries/README.md`](../../binaries/README.md).

Requires Rust 1.97 to build from source.

The current Unix manual page is [`docs/man/bn.1`](../../docs/man/bn.1). After installing `bn`
on the `PATH`, install the page with:

```shell
sudo install -m 644 docs/man/bn.1 /usr/share/man/man1/bn.1
man bn
```

From the source tree, without installing:

```shell
man docs/man/bn.1
```

## Commands

```shell
bn check examples/hello.bn
bn run examples/hello.bn
bn run examples/language-tour.bn
bn lex examples/hello.bn
bn check --emit ir examples/factorial.bn
bn run examples/hello.bn -- extra-arg
```

| Command | Effect |
| --- | --- |
| `check` | Lexer, parser, and semantics. Exit `0` if valid, `1` on a BN error, `2` on invalid tool use. |
| `run` | Check, lower to IR, execute `Start`. Exit code is `Start`'s result; BN errors are `1`, tool failures `2`. |
| `lex` | Print tokens. |

`HOST.Main.Argument(0)` is the source path given to `bn run`. Further arguments
come after `--`.

`-v` prints pipeline stages; `-v -v` or `-vv` also prints tokens.

Historical command behavior is preserved above. Current command reference:
[`bn(1)`](../../docs/man/bn.1). Current tutorial:
[`docs/book/en/toc.md`](../../docs/book/en/toc.md).

## First program

```basic
FUNCTION Start() AS VOID
    PRINT "Basic Next"
END FUNCTION
```

Save as `hello.bn` and run `bn run hello.bn`.

## Limits of 0.1

- Interpreter only. LLVM / `bn build` is post-0.1.
- `TIMEZONE` stores an IANA identifier; UTC conversions do not apply zone rules.
- `HOST.Console` has `CLS` and `BEEP`, not cursor addressing.
- No package registry, files, network, or GPU capabilities.

## Troubleshooting

| Symptom | What to try |
| --- | --- |
| `NAME_NOT_FOUND` on an imported function | Use the `IMPORT ... AS alias` name: `alias.member`. |
| `HOST.clock` / `HOST.main` rejected | Spell `HOST.Clock` and `HOST.Main`. |
| macOS refuses a downloaded binary | Allow it in System Settings; the 0.1 builds are unsigned. |
| `INPUT()` returns `EOF` | Provide a line on stdin; empty stdin is end of input. |
| Check passed but you expected execution | Use `bn run`, not `bn check`. |
