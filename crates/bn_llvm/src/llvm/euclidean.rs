#![allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_euclidean_integer_op(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let dest = destination.0;
    let left_op = coerce_to_type(text, left, left_ty, ty);
    let right_op = coerce_to_type(text, right, right_ty, ty);
    let zero_ok = take_continuation(block_id, state);
    let _ = writeln!(text, "  %divz{dest} = icmp eq {llvm_ty} {right_op}, 0");
    let _ = writeln!(
        text,
        "  br i1 %divz{dest}, label %trap_numeric_overflow, label %{zero_ok}"
    );
    let _ = writeln!(text, "{zero_ok}:");
    state.needs_numeric_overflow_trap = true;
    if is_unsigned(ty) {
        let opcode = match operator {
            "DIV" => "udiv",
            "Percent" => "urem",
            _ => unreachable!("validated euclidean operator"),
        };
        let _ = writeln!(
            text,
            "  %v{dest} = {opcode} {llvm_ty} {left_op}, {right_op}"
        );
        return;
    }
    let min = signed_minimum(llvm_ty);
    let _ = writeln!(text, "  %divmin{dest} = icmp eq {llvm_ty} {left_op}, {min}");
    let _ = writeln!(text, "  %divneg{dest} = icmp eq {llvm_ty} {right_op}, -1");
    let _ = writeln!(
        text,
        "  %divovf{dest} = and i1 %divmin{dest}, %divneg{dest}"
    );
    match operator {
        "DIV" => {
            let ok = take_continuation(block_id, state);
            let _ = writeln!(
                text,
                "  br i1 %divovf{dest}, label %trap_numeric_overflow, label %{ok}"
            );
            let _ = writeln!(text, "{ok}:");
            emit_signed_div(text, dest, llvm_ty, &left_op, &right_op);
        }
        "Percent" => {
            let min_case = take_continuation(block_id, state);
            let ok = take_continuation(block_id, state);
            let join = take_continuation(block_id, state);
            let _ = writeln!(
                text,
                "  br i1 %divovf{dest}, label %{min_case}, label %{ok}"
            );
            let _ = writeln!(text, "{min_case}:");
            let _ = writeln!(text, "  br label %{join}");
            let _ = writeln!(text, "{ok}:");
            emit_signed_rem(text, dest, llvm_ty, &left_op, &right_op);
            let _ = writeln!(text, "  br label %{join}");
            let _ = writeln!(text, "{join}:");
            let _ = writeln!(
                text,
                "  %v{dest} = phi {llvm_ty} [ 0, %{min_case} ], [ %eucl{dest}, %{ok} ]"
            );
        }
        _ => unreachable!("validated euclidean operator"),
    }
}

fn emit_signed_div(text: &mut String, dest: u32, llvm_ty: &str, left: &str, right: &str) {
    let _ = writeln!(text, "  %qtrunc{dest} = sdiv {llvm_ty} {left}, {right}");
    let _ = writeln!(text, "  %rtrunc{dest} = srem {llvm_ty} {left}, {right}");
    let _ = writeln!(text, "  %rneg{dest} = icmp slt {llvm_ty} %rtrunc{dest}, 0");
    let _ = writeln!(text, "  %rhspos{dest} = icmp sgt {llvm_ty} {right}, 0");
    let _ = writeln!(
        text,
        "  %qadj{dest} = select i1 %rhspos{dest}, {llvm_ty} -1, {llvm_ty} 1"
    );
    let _ = writeln!(
        text,
        "  %qfix{dest} = add {llvm_ty} %qtrunc{dest}, %qadj{dest}"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = select i1 %rneg{dest}, {llvm_ty} %qfix{dest}, {llvm_ty} %qtrunc{dest}"
    );
}

fn emit_signed_rem(text: &mut String, dest: u32, llvm_ty: &str, left: &str, right: &str) {
    let _ = writeln!(text, "  %rtrunc{dest} = srem {llvm_ty} {left}, {right}");
    let _ = writeln!(text, "  %rneg{dest} = icmp slt {llvm_ty} %rtrunc{dest}, 0");
    let _ = writeln!(text, "  %rhsneg{dest} = icmp slt {llvm_ty} {right}, 0");
    let _ = writeln!(text, "  %rsub{dest} = sub {llvm_ty} %rtrunc{dest}, {right}");
    let _ = writeln!(text, "  %radd{dest} = add {llvm_ty} %rtrunc{dest}, {right}");
    let _ = writeln!(
        text,
        "  %radj{dest} = select i1 %rhsneg{dest}, {llvm_ty} %rsub{dest}, {llvm_ty} %radd{dest}"
    );
    let _ = writeln!(
        text,
        "  %eucl{dest} = select i1 %rneg{dest}, {llvm_ty} %radj{dest}, {llvm_ty} %rtrunc{dest}"
    );
}

fn take_continuation(block_id: BlockId, state: &mut EmissionState) -> String {
    let name = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    name
}

fn signed_minimum(llvm_ty: &str) -> &'static str {
    match llvm_ty {
        "i8" => "-128",
        "i16" => "-32768",
        "i32" => "-2147483648",
        "i64" => "-9223372036854775808",
        _ => unreachable!("signed integer LLVM type"),
    }
}
