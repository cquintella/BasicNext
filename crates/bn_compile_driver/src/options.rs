//! Compiler-only option semantics: `--target`, `--opt`, `-g`, `--cpu` and `--emit`, parsed as a
//! `bn_cli::options::OptionExtension` supplied by the executable.

use bn_cli::options::{Emit, OptionExtension};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Native,
    Wasm32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Optimization {
    None,
    Level(u8),
    Size,
}

impl Optimization {
    #[must_use]
    pub const fn clang_flag(self) -> &'static str {
        match self {
            Self::None => "-O0",
            Self::Level(level) => match level {
                1 => "-O1",
                3 => "-O3",
                _ => "-O2",
            },
            Self::Size => "-Oz",
        }
    }

    #[must_use]
    pub const fn linker_flag(self) -> &'static str {
        match self {
            Self::None => "-O0",
            Self::Level(level) => match level {
                1 => "-O1",
                3 => "-O3",
                _ => "-O2",
            },
            Self::Size => "-O2",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CpuTarget {
    Generic,
    Native,
}

/// The compiler's own options (`--target`, `--opt`, `-g`, `--cpu`, `--emit`).
/// Only `bnc` parses them; to `bni` each is an unknown option (known issue K3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildOptions {
    pub target: Target,
    pub optimization: Optimization,
    /// `-g`: emit source-level debug information.
    pub debug: bool,
    /// `--cpu native|generic`: opt-in to host CPU-specific instructions.
    pub cpu: CpuTarget,
    /// `--emit`: print a frontend artifact or LLVM IR instead of compiling.
    pub emit: Option<Emit>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            optimization: Optimization::Level(2),
            debug: false,
            cpu: CpuTarget::Generic,
            emit: None,
        }
    }
}

impl OptionExtension for BuildOptions {
    fn accept(
        &mut self,
        argument: &str,
        rest: &mut dyn Iterator<Item = String>,
    ) -> Result<bool, String> {
        match argument {
            "-g" | "--debug" => self.debug = true,
            "--opt" => {
                self.optimization = match rest.next().as_deref() {
                    Some("none") => Optimization::None,
                    Some("1") => Optimization::Level(1),
                    Some("2") => Optimization::Level(2),
                    Some("3") => Optimization::Level(3),
                    Some("s") => Optimization::Size,
                    _ => return Err("--opt expects none, 1, 2, 3, or s".into()),
                };
            }
            "--target" => {
                self.target = match rest.next().as_deref() {
                    Some("native") => Target::Native,
                    Some("wasm32") => Target::Wasm32,
                    _ => return Err("--target expects native or wasm32".into()),
                };
            }
            "--cpu" => {
                self.cpu = match rest.next().as_deref() {
                    Some("generic") => CpuTarget::Generic,
                    Some("native") => CpuTarget::Native,
                    _ => return Err("--cpu expects generic or native".into()),
                };
            }
            "--emit" => {
                if self.emit.is_some() {
                    return Err(bn_catalog::text("cli", "option.emit.repeated").into());
                }
                self.emit = Some(match rest.next().as_deref() {
                    Some("tokens") => Emit::Tokens,
                    Some("ast") => Emit::Ast,
                    Some("typed-ast") => Emit::TypedAst,
                    Some("ir") => Emit::Ir,
                    Some("llvm" | "llvm-ir") => Emit::Llvm,
                    _ => return Err(bn_catalog::text("cli", "option.emit.expects").into()),
                });
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
