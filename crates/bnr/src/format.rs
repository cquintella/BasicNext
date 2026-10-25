// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Structured terminal data rendering for Basic Next REPL (`bnr`).
//!
//! Provides tabular formatting for `BNData.DataFrame`, pretty-printed JSON
//! serialization for `BNJson`, field inspection for `STRUCT`/`CLASS` instances,
//! and indexed previews for large vectors.

use std::fmt::Write as _;

use bn_value::Value;

const MAX_PREVIEW_ROWS: usize = 10;
const MAX_INLINE_VECTOR_LEN: usize = 6;

/// Formats a runtime value into a structured preview if applicable.
#[must_use]
pub fn format_structured_value(value: &Value, _type_name: &str) -> Option<String> {
    match value {
        Value::Vector(items) => Some(format_vector(items)),
        Value::Record { record } => Some(format_record(record)),
        Value::Object { class, handle } => Some(format_object(class, handle.slot)),
        _ => None,
    }
}

/// Formats a vector into an indexed or inline preview.
#[must_use]
pub fn format_vector(items: &[Value]) -> String {
    if items.len() <= MAX_INLINE_VECTOR_LEN {
        let elements = items
            .iter()
            .map(bn_interp::render_pub)
            .collect::<Vec<_>>()
            .join(", ");
        return format!("[{elements}]");
    }

    let mut output = String::new();
    let _ = writeln!(output, "[");
    for (idx, item) in items.iter().take(MAX_PREVIEW_ROWS).enumerate() {
        let rendered = bn_interp::render_pub(item);
        let _ = writeln!(output, "  [{idx}]: {rendered}");
    }
    if items.len() > MAX_PREVIEW_ROWS {
        let total = items.len().to_string();
        let more_msg = bn_catalog::fill("cli", "msg.bnr_vector_more", &[("total", &total)]);
        let _ = writeln!(output, "{more_msg}");
    }
    output.push(']');
    output
}

/// Formats a record/STRUCT instance with field inspection.
#[must_use]
pub fn format_record(record: &bn_value::RecordValue) -> String {
    let mut output = String::new();
    let type_name = record.type_name();
    let _ = write!(output, "{type_name} {{ ");
    let mut fields = Vec::with_capacity(record.len());
    for slot in 0..record.len() {
        if let Some(val) = record.get(slot) {
            fields.push(bn_interp::render_pub(val));
        }
    }
    output.push_str(&fields.join(", "));
    output.push_str(" }");
    output
}

/// Formats an Object/CLASS reference.
#[must_use]
pub fn format_object(class: &str, slot: u32) -> String {
    format!("{class} (instance #{slot})")
}

/// Formats pretty JSON string from raw text.
#[must_use]
pub fn format_pretty_json(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}

/// Formats a tabular preview of `DataFrame` columns and rows.
#[must_use]
pub fn format_dataframe_table(
    col_names: &[String],
    col_types: &[String],
    rows: &[Vec<String>],
    total_rows: usize,
    total_cols: usize,
) -> String {
    if col_names.is_empty() {
        let rows_str = total_rows.to_string();
        let cols_str = total_cols.to_string();
        return bn_catalog::fill(
            "cli",
            "msg.bnr_dataframe_dims",
            &[("rows", &rows_str), ("cols", &cols_str)],
        );
    }

    let mut col_widths = Vec::with_capacity(col_names.len());
    for (i, name) in col_names.iter().enumerate() {
        let ty = col_types.get(i).map_or("ANY", String::as_str);
        let header_len = name.len() + ty.len() + 3; // "name [ty]"
        let mut max_w = header_len;
        for row in rows {
            if let Some(cell) = row.get(i) {
                max_w = max_w.max(cell.len());
            }
        }
        col_widths.push(max_w);
    }

    let mut output = String::new();

    // Header line
    output.push_str("  ");
    for (i, name) in col_names.iter().enumerate() {
        let ty = col_types.get(i).map_or("ANY", String::as_str);
        let header = format!("{name} [{ty}]");
        let w = col_widths[i];
        let _ = write!(output, "{header:<w$}");
        if i + 1 < col_names.len() {
            output.push_str(" | ");
        }
    }
    output.push('\n');

    // Separator line
    output.push_str("  ");
    for (i, &w) in col_widths.iter().enumerate() {
        output.push_str(&"-".repeat(w));
        if i + 1 < col_widths.len() {
            output.push_str("-+-");
        }
    }
    output.push('\n');

    // Rows
    for row in rows {
        output.push_str("  ");
        for (i, cell) in row.iter().enumerate() {
            let w = col_widths[i];
            let _ = write!(output, "{cell:<w$}");
            if i + 1 < row.len() {
                output.push_str(" | ");
            }
        }
        output.push('\n');
    }

    // Dimensions footer
    let rows_str = total_rows.to_string();
    let cols_str = total_cols.to_string();
    let dims = bn_catalog::fill(
        "cli",
        "msg.bnr_dataframe_dims",
        &[("rows", &rows_str), ("cols", &cols_str)],
    );
    let _ = write!(output, "  {dims}");
    output
}
