use std::process::ExitCode;

use super::{VERSION, tool_error};

pub(crate) fn help() -> ExitCode {
    println!(
        "\
{VERSION}
usage: bn <check|lex|run|build|lsp|dap> [options] <file.bn> [-- program-args]

commands:
  check   validate lexer, parser, and semantics
  lex     print the token stream
  run     execute FUNCTION Start through typed BN IR
  build   compile the supported typed BN IR subset with LLVM
  lsp     serve Language Server Protocol over stdio
  dap     serve Debug Adapter Protocol over stdio

options:
  -v, --verbose              show pipeline stages (repeat for tokens: -v -v)
  -vv                        show stages and tokens
  --emit tokens|ast|typed-ast|ir
                             print a frontend artifact (use with check or lex)
  -o, --output <file>        write an emitted artifact to <file>
  --trace                    report the bn run execution entry point
  --target native|wasm32     select the build target (build only)
  --opt none|1|2|3|s         optimization level for native/Wasm builds (default 2)
  --no-filesystem            deny HOST.FileSystem imports (run only)
  --warnings errors           promote all warning diagnostics to errors
  --allow <CODE>              suppress one warning diagnostic (repeatable)
  --warn <CODE>               keep one warning diagnostic at warning level
  --deny <CODE>               promote one warning diagnostic to an error
  --color auto|always|never  control ANSI color on status messages
  --log-level error|warn|info|debug
                             process log verbosity (build)
  --log-file <file>          companion process log path (build)
  --no-log                   disable the companion process log
  --config <file>            load warning/logging configuration
  -V, --version              print version
  -h, --help                 print this help

HOST.Args[0] is the source path. Extra program arguments follow --.
See also: man bn
"
    );
    ExitCode::SUCCESS
}

pub(crate) fn usage() -> ExitCode {
    eprintln!("usage: bn <check|lex|run|build|lsp|dap> [options] <file.bn>\ntry: bn --help");
    tool_error()
}
