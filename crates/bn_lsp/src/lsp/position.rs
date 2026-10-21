//! Bidirectional canonical position conversions between Basic Next character
//! columns and LSP UTF-16 code units, handling surrogate pairs and NULs safely.

use bn_source::{SourceFile, Span};
use lsp_types::{Position as LspPosition, Range as LspRange};

/// Converts a 1-based character (scalar) column on `line` to a 0-based UTF-16
/// code unit offset for LSP.
#[must_use]
pub(crate) fn scalar_to_utf16_col(line: &str, scalar_col_1based: usize) -> u32 {
    let target_scalar = scalar_col_1based.saturating_sub(1);
    let mut utf16_offset: u32 = 0;
    for (idx, ch) in line.chars().enumerate() {
        if idx >= target_scalar {
            break;
        }
        let ch_utf16 = u32::try_from(ch.len_utf16()).unwrap_or(1);
        utf16_offset = utf16_offset.saturating_add(ch_utf16);
    }
    utf16_offset
}

/// Converts a 0-based UTF-16 code unit offset on `line` to a 1-based character
/// (scalar) column for Basic Next positions.
#[must_use]
pub(crate) fn utf16_to_scalar_col(line: &str, utf16_col_0based: u32) -> usize {
    let mut accumulated_utf16: u32 = 0;
    let mut scalar_idx: usize = 0;
    for ch in line.chars() {
        if accumulated_utf16 >= utf16_col_0based {
            break;
        }
        let ch_utf16 = u32::try_from(ch.len_utf16()).unwrap_or(1);
        accumulated_utf16 = accumulated_utf16.saturating_add(ch_utf16);
        scalar_idx += 1;
    }
    scalar_idx + 1
}

/// Converts a Basic Next `Span` to an LSP `Range` using UTF-16 code units.
#[must_use]
pub(crate) fn span_to_lsp_range(source: &SourceFile, span: Span) -> LspRange {
    let start_line_idx = span.start.line.saturating_sub(1);
    let end_line_idx = span.end.line.saturating_sub(1);

    let start_line_text = source.line(span.start.line);
    let end_line_text = source.line(span.end.line);

    let start_character = scalar_to_utf16_col(start_line_text, span.start.column);
    let end_character = scalar_to_utf16_col(end_line_text, span.end.column);

    LspRange {
        start: LspPosition::new(
            u32::try_from(start_line_idx).unwrap_or(u32::MAX),
            start_character,
        ),
        end: lsp_range_end(end_line_idx, end_character),
    }
}

fn lsp_range_end(line_idx: usize, character: u32) -> LspPosition {
    LspPosition::new(u32::try_from(line_idx).unwrap_or(u32::MAX), character)
}

/// Converts an LSP `Position` to Basic Next 1-based `(line, column)` scalar coordinates.
#[must_use]
pub fn lsp_pos_to_scalar(source: &SourceFile, pos: LspPosition) -> (usize, usize) {
    let line_1based = (pos.line as usize).saturating_add(1);
    let line_text = source.line(line_1based);
    let col_1based = utf16_to_scalar_col(line_text, pos.character);
    (line_1based, col_1based)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_columns_map_one_to_one() {
        let line = "PRINT 42";
        assert_eq!(scalar_to_utf16_col(line, 1), 0);
        assert_eq!(scalar_to_utf16_col(line, 7), 6);
        assert_eq!(utf16_to_scalar_col(line, 0), 1);
        assert_eq!(utf16_to_scalar_col(line, 6), 7);
    }

    #[test]
    fn surrogate_pairs_emojis_advance_two_utf16_units_per_scalar() {
        let line = "LET 🚀 = 1";
        // 'L'=1, 'E'=2, 'T'=3, ' '=4, '🚀'=5, ' '=6
        assert_eq!(scalar_to_utf16_col(line, 1), 0);
        assert_eq!(scalar_to_utf16_col(line, 5), 4);
        // After 🚀 (1 char, 2 utf16 units): character 6 is at utf16 offset 6
        assert_eq!(scalar_to_utf16_col(line, 6), 6);

        // Reverse
        assert_eq!(utf16_to_scalar_col(line, 0), 1);
        assert_eq!(utf16_to_scalar_col(line, 4), 5);
        assert_eq!(utf16_to_scalar_col(line, 6), 6);
    }

    #[test]
    fn embedded_nul_characters_are_preserved() {
        let line = "A\0B";
        assert_eq!(scalar_to_utf16_col(line, 1), 0);
        assert_eq!(scalar_to_utf16_col(line, 2), 1); // at \0
        assert_eq!(scalar_to_utf16_col(line, 3), 2); // at B
        assert_eq!(utf16_to_scalar_col(line, 2), 3);
    }
}
