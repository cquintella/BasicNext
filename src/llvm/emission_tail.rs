#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_scalar_instruction_tail(
    text: &mut String,
    module: &Module,
    function: &Function,
    _block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    match instruction {
        Instruction::Call {
            destination,
            callee,
            arguments,
            ..
        } => match analysis
            .functions
            .get(callee)
            .copied()
            .expect("validated callee")
        {
            "HOST.Random.Seed" => {
                let seed = extend_to_i64(
                    text,
                    *arguments.first().expect("validated seed argument"),
                    analysis
                        .values
                        .get(arguments.first().expect("validated seed argument"))
                        .expect("validated seed type"),
                );
                let _ = writeln!(text, "  %rngzero{} = icmp eq i64 {seed}, 0", destination.0);
                let _ = writeln!(
                    text,
                    "  %rngseed{} = select i1 %rngzero{}, i64 1, i64 {seed}",
                    destination.0, destination.0
                );
                let _ = writeln!(text, "  store i64 %rngseed{}, ptr %rng", destination.0);
            }
            "HOST.Random.Random" => {
                let _ = writeln!(text, "  %rng0{} = load i64, ptr %rng", destination.0);
                let _ = writeln!(
                    text,
                    "  %rng1{} = lshr i64 %rng0{}, 12",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng2{} = xor i64 %rng0{}, %rng1{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng3{} = shl i64 %rng2{}, 25",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng4{} = xor i64 %rng2{}, %rng3{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng5{} = lshr i64 %rng4{}, 27",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng6{} = xor i64 %rng4{}, %rng5{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rngnext{} = mul i64 %rng6{}, 2685821657736338717",
                    destination.0, destination.0
                );
                let _ = writeln!(text, "  store i64 %rngnext{}, ptr %rng", destination.0);
                let _ = writeln!(
                    text,
                    "  %rngscale{} = lshr i64 %rngnext{}, 11",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rngfloat{} = uitofp i64 %rngscale{} to double",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %v{} = fdiv double %rngfloat{}, 9007199254740992.0",
                    destination.0, destination.0
                );
            }
            _ => unreachable!("validated call target"),
        },
        Instruction::Input { destination, .. } => {
            block_state.constants.remove(destination);
            let _ = writeln!(text, "  %v{} = call ptr @bn_input()", destination.0);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let _ = writeln!(text, "  %v{} = add i32 0, %argc", destination.0);
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            match llvm_type(analysis.values.get(index).expect("validated index type")) {
                Some("i32") => {
                    let _ = writeln!(
                        text,
                        "  %argptr{} = getelementptr ptr, ptr %argv, i32 %v{}",
                        destination.0, index.0
                    );
                }
                Some(other) => {
                    let cast =
                        if is_unsigned(analysis.values.get(index).expect("validated index type")) {
                            "zext"
                        } else {
                            "sext"
                        };
                    let _ = writeln!(
                        text,
                        "  %argindex{} = {cast} {other} %v{} to i32",
                        destination.0, index.0
                    );
                    let _ = writeln!(
                        text,
                        "  %argptr{} = getelementptr ptr, ptr %argv, i32 %argindex{}",
                        destination.0, destination.0
                    );
                }
                None => unreachable!("validated index type"),
            }
            let _ = writeln!(
                text,
                "  %v{} = load ptr, ptr %argptr{}",
                destination.0, destination.0
            );
        }
        Instruction::Print {
            values: printed, ..
        } => {
            for value in printed {
                lower_print_value(
                    text,
                    *value,
                    analysis
                        .values
                        .get(value)
                        .expect("validated printable type"),
                    state,
                );
            }
            let _ = writeln!(
                text,
                "  %newline{} = call i32 @putchar(i32 10)",
                state.print_count
            );
            state.print_count += 1;
        }
        _ => {
            return Err(unsupported_instruction(
                module,
                function,
                instruction,
                instruction_name(instruction),
            ));
        }
    }
    Ok(())
}
