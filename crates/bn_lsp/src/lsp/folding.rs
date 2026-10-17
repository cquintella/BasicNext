//! Folding range detection based on the Basic Next AST.

use lsp_types::{FoldingRange, FoldingRangeKind};

use bn_frontend::{
    ast::{Item, Program, Statement},
    lexer::lex,
    parser::parse_named,
};
use bn_source::SourceFile;

/// Computes folding ranges for Basic Next declaration blocks and control flow statements.
#[must_use]
pub fn compute_folding_ranges(source: &SourceFile) -> Vec<FoldingRange> {
    let Ok(tokens) = lex(source) else {
        return Vec::new();
    };
    let Ok(program) = parse_named(&tokens, source.name.clone()) else {
        return Vec::new();
    };
    collect_program_folding_ranges(&program)
}

fn collect_program_folding_ranges(program: &Program) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    for item in &program.items {
        collect_item_folding_ranges(item, &mut ranges);
    }
    ranges
}

fn collect_item_folding_ranges(item: &Item, ranges: &mut Vec<FoldingRange>) {
    match item {
        Item::Declaration {
            span, statements, ..
        } => {
            push_span_range(span, ranges);
            for stmt in statements {
                collect_statement_folding_ranges(stmt, ranges);
            }
        }
        Item::Constant { .. } | Item::Import { .. } => {}
    }
}

fn collect_statement_folding_ranges(stmt: &Statement, ranges: &mut Vec<FoldingRange>) {
    match stmt {
        Statement::If {
            span,
            branches,
            otherwise,
        } => {
            push_span_range(span, ranges);
            for branch in branches {
                for s in &branch.body.statements {
                    collect_statement_folding_ranges(s, ranges);
                }
            }
            if let Some(block) = otherwise {
                for s in &block.statements {
                    collect_statement_folding_ranges(s, ranges);
                }
            }
        }
        Statement::While { span, body, .. }
        | Statement::Repeat { span, body, .. }
        | Statement::For { span, body, .. } => {
            push_span_range(span, ranges);
            for s in &body.statements {
                collect_statement_folding_ranges(s, ranges);
            }
        }
        Statement::MemberFunction { span, body, .. } => {
            push_span_range(span, ranges);
            if let Some(block) = body {
                for s in &block.statements {
                    collect_statement_folding_ranges(s, ranges);
                }
            }
        }
        Statement::Binding { .. }
        | Statement::Destructuring { .. }
        | Statement::Assignment { .. }
        | Statement::Return { .. }
        | Statement::Print { .. }
        | Statement::ClearScreen { .. }
        | Statement::Beep { .. }
        | Statement::Release { .. }
        | Statement::Stop { .. }
        | Statement::Control { .. }
        | Statement::Call { .. } => {}
    }
}

fn push_span_range(span: &bn_source::Span, ranges: &mut Vec<FoldingRange>) {
    if span.end.line > span.start.line {
        ranges.push(FoldingRange {
            start_line: u32::try_from(span.start.line.saturating_sub(1)).unwrap_or(u32::MAX),
            start_character: None,
            end_line: u32::try_from(span.end.line.saturating_sub(1)).unwrap_or(u32::MAX),
            end_character: None,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_folding_ranges_from_ast() {
        let code = "FUNCTION Calc(x AS INTEGER) AS INTEGER\n    IF x > 0 THEN\n        RETURN x\n    END IF\n    RETURN 0\nEND FUNCTION\n";
        let source = SourceFile::new("calc.bn", code);
        let ranges = compute_folding_ranges(&source);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 5);
        assert_eq!(ranges[1].start_line, 1);
        assert_eq!(ranges[1].end_line, 3);
    }
}
