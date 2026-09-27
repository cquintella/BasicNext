//! Compiler-only option semantics: `--target` and `--opt`, parsed as a
//! `bn_cli::options::OptionExtension` supplied by the executable.

use bn_cli::options::OptionExtension;

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

/// Compile-only flags (`--target`, `--opt`). The compiler executable
/// parses them; `bn` also accepts them on every command for compatibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildOptions {
    pub target: Target,
    pub optimization: Optimization,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            optimization: Optimization::Level(2),
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
            _ => return Ok(false),
        }
        Ok(true)
    }
}
