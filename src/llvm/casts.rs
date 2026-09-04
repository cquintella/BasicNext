#![allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_cast(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source_ty: &Type,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    match (llvm_type(source_ty), llvm_type(target_ty)) {
        (Some(source), Some("i1")) => emit_to_boolean(text, destination, value, source),
        (Some(source), Some(target)) if integer_llvm(source) && integer_llvm(target) => {
            emit_integer_to_integer(
                text,
                block_id,
                destination,
                value,
                source,
                target,
                source_ty,
                target_ty,
                state,
            );
        }
        (Some(source), Some(target)) if integer_llvm(source) && float_llvm(target) => {
            let opcode = if is_unsigned(source_ty) {
                "uitofp"
            } else {
                "sitofp"
            };
            let _ = writeln!(
                text,
                "  %v{} = {opcode} {source} %v{} to {target}",
                destination.0, value.0
            );
        }
        (Some(source), Some(target)) if float_llvm(source) && integer_llvm(target) => {
            emit_float_to_integer(
                text,
                block_id,
                destination,
                value,
                source,
                target,
                target_ty,
                state,
            );
        }
        (Some("float"), Some("double")) => {
            let _ = writeln!(
                text,
                "  %v{} = fpext float %v{} to double",
                destination.0, value.0
            );
        }
        (Some("double"), Some("float")) => {
            let _ = writeln!(
                text,
                "  %v{} = fptrunc double %v{} to float",
                destination.0, value.0
            );
        }
        (Some("ptr"), Some("ptr"))
        | (Some("float"), Some("float"))
        | (Some("double"), Some("double")) => {
            emit_same_type_copy(text, destination, value, target_ty);
        }
        _ => unreachable!("validated cast shape"),
    }
}

fn emit_same_type_copy(text: &mut String, destination: ValueId, value: ValueId, ty: &Type) {
    match llvm_type(ty).expect("validated copy type") {
        "i1" => {
            let _ = writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, value.0);
        }
        "ptr" => {
            let _ = writeln!(
                text,
                "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                destination.0, value.0
            );
        }
        "float" => {
            let _ = writeln!(
                text,
                "  %v{} = fadd float 0.0, %v{}",
                destination.0, value.0
            );
        }
        "double" => {
            let _ = writeln!(
                text,
                "  %v{} = fadd double 0.0, %v{}",
                destination.0, value.0
            );
        }
        other => {
            let _ = writeln!(text, "  %v{} = add {other} 0, %v{}", destination.0, value.0);
        }
    }
}

fn emit_to_boolean(text: &mut String, destination: ValueId, value: ValueId, source: &str) {
    match source {
        "i1" => {
            let _ = writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, value.0);
        }
        "i8" | "i16" | "i32" | "i64" => {
            let _ = writeln!(
                text,
                "  %v{} = icmp ne {source} %v{}, 0",
                destination.0, value.0
            );
        }
        "float" => {
            let _ = writeln!(
                text,
                "  %v{} = fcmp une float %v{}, 0.0",
                destination.0, value.0
            );
        }
        "double" => {
            let _ = writeln!(
                text,
                "  %v{} = fcmp une double %v{}, 0.0",
                destination.0, value.0
            );
        }
        "ptr" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %boolch{dest} = load i8, ptr %v{}", value.0);
            let _ = writeln!(text, "  %v{dest} = icmp ne i8 %boolch{dest}, 0");
        }
        _ => unreachable!("validated boolean cast source"),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_integer_to_integer(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source: &str,
    target: &str,
    source_ty: &Type,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let ext = if is_unsigned(source_ty) {
        "zext"
    } else {
        "sext"
    };
    let _ = writeln!(
        text,
        "  %castw{dest} = {ext} {source} %v{} to i128",
        value.0
    );
    emit_i128_fit_trunc(text, block_id, dest, target, target_ty, state);
}

#[allow(clippy::too_many_arguments)]
fn emit_float_to_integer(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source: &str,
    target: &str,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %castnan{dest} = fcmp uno {source} %v{}, 0.0",
        value.0
    );
    let inf = "0x7FF0000000000000";
    let ninf = "0xFFF0000000000000";
    let _ = writeln!(
        text,
        "  %castpinf{dest} = fcmp oeq {source} %v{}, {inf}",
        value.0
    );
    let _ = writeln!(
        text,
        "  %castninf{dest} = fcmp oeq {source} %v{}, {ninf}",
        value.0
    );
    let _ = writeln!(
        text,
        "  %castinf{dest} = or i1 %castpinf{dest}, %castninf{dest}"
    );
    let _ = writeln!(
        text,
        "  %castbad{dest} = or i1 %castnan{dest}, %castinf{dest}"
    );
    let finite = take_continuation(block_id, state);
    let _ = writeln!(
        text,
        "  br i1 %castbad{dest}, label %trap_numeric_overflow, label %{finite}"
    );
    let _ = writeln!(text, "{finite}:");
    state.needs_numeric_overflow_trap = true;
    let _ = writeln!(
        text,
        "  %castw{dest} = fptosi {source} %v{} to i128",
        value.0
    );
    emit_i128_fit_trunc(text, block_id, dest, target, target_ty, state);
}

fn emit_i128_fit_trunc(
    text: &mut String,
    block_id: BlockId,
    dest: u32,
    target: &str,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let (min, max) = i128_bounds(target_ty);
    let _ = writeln!(text, "  %castlo{dest} = icmp slt i128 %castw{dest}, {min}");
    let _ = writeln!(text, "  %casthi{dest} = icmp sgt i128 %castw{dest}, {max}");
    let _ = writeln!(text, "  %castov{dest} = or i1 %castlo{dest}, %casthi{dest}");
    let ok = take_continuation(block_id, state);
    let _ = writeln!(
        text,
        "  br i1 %castov{dest}, label %trap_numeric_overflow, label %{ok}"
    );
    let _ = writeln!(text, "{ok}:");
    let _ = writeln!(text, "  %v{dest} = trunc i128 %castw{dest} to {target}");
    state.needs_numeric_overflow_trap = true;
}

fn take_continuation(block_id: BlockId, state: &mut EmissionState) -> String {
    let name = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    name
}

fn i128_bounds(ty: &Type) -> (&'static str, &'static str) {
    match integer_kind(ty) {
        IntegerType::Byte => ("0", "255"),
        IntegerType::Int8 => ("-128", "127"),
        IntegerType::Int16 => ("-32768", "32767"),
        IntegerType::Int32 => ("-2147483648", "2147483647"),
        IntegerType::Int64 => ("-9223372036854775808", "9223372036854775807"),
        IntegerType::UInt16 => ("0", "65535"),
        IntegerType::UInt32 => ("0", "4294967295"),
        IntegerType::UInt64 => ("0", "18446744073709551615"),
    }
}
