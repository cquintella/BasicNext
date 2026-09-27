#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

pub(crate) fn lower_print_emission(
    text: &mut String,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::Print {
            values: printed, ..
        } => {
            let stdout = format!("%stdout{}", state.print_count);
            if state.synchronize_prints {
                let stdout_sym = crate::helpers::stdout_file_symbol();
                let _ = writeln!(text, "  {stdout} = load ptr, ptr @{stdout_sym}");
                let _ = writeln!(text, "  call void @flockfile(ptr {stdout})");
            }
            for (index, value) in printed.iter().enumerate() {
                if index > 0 {
                    let _ = writeln!(
                        text,
                        "  %separator{} = call i32 @putchar(i32 32)",
                        state.print_count
                    );
                    state.print_count += 1;
                }
                lower_print_value(
                    text,
                    *value,
                    analysis
                        .values
                        .get(value)
                        .expect("validated printable type"),
                    state,
                );
                if analysis.owned_string_results.contains(value) {
                    let id = value.0;
                    let _ = writeln!(
                        text,
                        "  %ownedstringerror{id} = extractvalue {{ i1, ptr, i64 }} %v{id}, 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringptr{id} = extractvalue {{ i1, ptr, i64 }} %v{id}, 1"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringfree{id} = select i1 %ownedstringerror{id}, ptr null, ptr %ownedstringptr{id}"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringna{id} = icmp eq ptr %ownedstringfree{id}, @.bn_na"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringstorage{id} = select i1 %ownedstringna{id}, ptr null, ptr %ownedstringfree{id}"
                    );
                    let _ = writeln!(text, "  call void @free(ptr %ownedstringstorage{id})");
                }
            }
            let _ = writeln!(
                text,
                "  %newline{} = call i32 @putchar(i32 10)",
                state.print_count
            );
            state.print_count += 1;
            if state.synchronize_prints {
                let _ = writeln!(text, "  call void @funlockfile(ptr {stdout})");
            }
        }
        _ => return false,
    }
    true
}
