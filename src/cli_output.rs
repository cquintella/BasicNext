use std::{
    fmt,
    io::{self, IsTerminal},
};

use super::Color;
pub(crate) fn color_enabled(color: Color) -> bool {
    matches!(color, Color::Always) || (matches!(color, Color::Auto) && io::stdout().is_terminal())
}

pub(crate) fn colorize(text: &str, color: Color) -> String {
    if color_enabled(color) {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.into()
    }
}

pub(crate) fn log(verbosity: u8, level: u8, message: impl fmt::Display) {
    if verbosity >= level {
        eprintln!("[bn] {message}");
    }
}

pub(crate) fn module_index(id: u32) -> usize {
    usize::try_from(id).unwrap_or(usize::MAX)
}
