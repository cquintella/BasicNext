#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn emit_integer_not(
    text: &mut String,
    destination: ValueId,
    operand: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    if is_unsigned(ty) {
        let cont = format!("bnot{}.dead", destination.0);
        let _ = writeln!(
            text,
            "  br i1 true, label %trap_numeric_overflow, label %{cont}"
        );
        let _ = writeln!(text, "{cont}:");
        let _ = writeln!(text, "  %v{} = add {llvm_ty} 0, 0", destination.0);
        state.needs_numeric_overflow_trap = true;
        return;
    }
    let _ = writeln!(
        text,
        "  %v{} = xor {llvm_ty} %v{}, -1",
        destination.0, operand.0
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_shift(
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
    let dest = destination.0;
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let left_llvm = llvm_type(left_ty).expect("validated shift left type");
    let right_llvm = llvm_type(right_ty).expect("validated shift count type");
    let width = bit_width(ty);
    emit_cast_integer(
        text,
        &format!("shcnt{dest}"),
        &format!("%v{}", right.0),
        right_llvm,
        "i64",
        extend_op(right_ty),
    );
    if is_unsigned(right_ty) {
        let _ = writeln!(text, "  %shneg{dest} = or i1 false, false");
    } else {
        let _ = writeln!(text, "  %shneg{dest} = icmp slt i64 %shcnt{dest}, 0");
    }
    let _ = writeln!(text, "  %shwide{dest} = icmp uge i64 %shcnt{dest}, {width}");
    let _ = writeln!(text, "  %shbad{dest} = or i1 %shneg{dest}, %shwide{dest}");
    let ok = take_continuation(block_id, state);
    let _ = writeln!(
        text,
        "  br i1 %shbad{dest}, label %trap_numeric_overflow, label %{ok}"
    );
    let _ = writeln!(text, "{ok}:");
    state.needs_numeric_overflow_trap = true;
    let _ = writeln!(text, "  %shamt{dest} = zext i64 %shcnt{dest} to i128");
    if operator == "SHR" {
        let shift_left = if left_llvm == llvm_ty {
            format!("%v{}", left.0)
        } else {
            let narrow = format!("shnarrow{dest}");
            let _ = writeln!(text, "  %{narrow} = trunc {left_llvm} %v{} to {llvm_ty}", left.0);
            format!("%{narrow}")
        };
        emit_cast_integer(
            text,
            &format!("shbits{dest}"),
            &shift_left,
            llvm_ty,
            "i128",
            "zext",
        );
        let _ = writeln!(
            text,
            "  %shraw{dest} = lshr i128 %shbits{dest}, %shamt{dest}"
        );
        let _ = writeln!(text, "  %v{dest} = trunc i128 %shraw{dest} to {llvm_ty}");
        return;
    }
    emit_cast_integer(
        text,
        &format!("shval{dest}"),
        &format!("%v{}", left.0),
        left_llvm,
        "i128",
        extend_op(left_ty),
    );
    let _ = writeln!(text, "  %shraw{dest} = shl i128 %shval{dest}, %shamt{dest}");
    emit_i128_range_trunc(text, block_id, dest, llvm_ty, ty, state);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_integer_power(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let left_llvm = llvm_type(left_ty).expect("validated power base type");
    let right_llvm = llvm_type(right_ty).expect("validated power exponent type");
    emit_cast_integer(
        text,
        &format!("pbase{dest}"),
        &format!("%v{}", left.0),
        left_llvm,
        "i128",
        extend_op(left_ty),
    );
    emit_cast_integer(
        text,
        &format!("pexp{dest}"),
        &format!("%v{}", right.0),
        right_llvm,
        "i128",
        extend_op(right_ty),
    );
    let _ = writeln!(text, "  %pneg{dest} = icmp slt i128 %pexp{dest}, 0");
    let _ = writeln!(
        text,
        "  %pbig{dest} = icmp ugt i128 %pexp{dest}, 4294967295"
    );
    let _ = writeln!(text, "  %pbad{dest} = or i1 %pneg{dest}, %pbig{dest}");
    let setup = take_continuation(block_id, state);
    let _ = writeln!(
        text,
        "  br i1 %pbad{dest}, label %trap_numeric_overflow, label %{setup}"
    );
    let _ = writeln!(text, "{setup}:");
    state.needs_numeric_overflow_trap = true;
    let loop_h = format!("b{}.pow{dest}.loop", block_id.0);
    let work = format!("b{}.pow{dest}.work", block_id.0);
    let mulr = format!("b{}.pow{dest}.mulr", block_id.0);
    let after = format!("b{}.pow{dest}.after", block_id.0);
    let square = format!("b{}.pow{dest}.sq", block_id.0);
    let done = format!("b{}.pow{dest}.done", block_id.0);
    let _ = writeln!(text, "  br label %{loop_h}");
    let _ = writeln!(text, "{loop_h}:");
    let square_ok = format!("{square}.ok");
    let mulr_ok = format!("{mulr}.ok");
    let _ = writeln!(
        text,
        "  %pb{dest} = phi i128 [ %pbase{dest}, %{setup} ], [ %pb2{dest}, %{square_ok} ]"
    );
    let _ = writeln!(
        text,
        "  %pe{dest} = phi i128 [ %pexp{dest}, %{setup} ], [ %pe1{dest}, %{square_ok} ]"
    );
    let _ = writeln!(
        text,
        "  %pr{dest} = phi i128 [ 1, %{setup} ], [ %pr2{dest}, %{square_ok} ]"
    );
    let _ = writeln!(text, "  %pez{dest} = icmp eq i128 %pe{dest}, 0");
    let _ = writeln!(text, "  br i1 %pez{dest}, label %{done}, label %{work}");
    let _ = writeln!(text, "{work}:");
    let _ = writeln!(text, "  %podd{dest} = trunc i128 %pe{dest} to i1");
    let _ = writeln!(text, "  br i1 %podd{dest}, label %{mulr}, label %{after}");
    let _ = writeln!(text, "{mulr}:");
    emit_checked_i128_mul(text, dest, "pr", "pb", "prm", &mulr);
    let _ = writeln!(text, "  br label %{after}");
    let _ = writeln!(text, "{after}:");
    let _ = writeln!(
        text,
        "  %pr2{dest} = phi i128 [ %prm{dest}, %{mulr_ok} ], [ %pr{dest}, %{work} ]"
    );
    let _ = writeln!(text, "  %pe1{dest} = lshr i128 %pe{dest}, 1");
    let _ = writeln!(text, "  %pmore{dest} = icmp ne i128 %pe1{dest}, 0");
    let _ = writeln!(text, "  br i1 %pmore{dest}, label %{square}, label %{done}");
    let _ = writeln!(text, "{square}:");
    emit_checked_i128_mul(text, dest, "pb", "pb", "pb2", &square);
    let _ = writeln!(text, "  br label %{loop_h}");
    let _ = writeln!(text, "{done}:");
    let _ = writeln!(
        text,
        "  %shraw{dest} = phi i128 [ %pr{dest}, %{loop_h} ], [ %pr2{dest}, %{after} ]"
    );
    emit_i128_range_trunc(text, block_id, dest, llvm_ty, ty, state);
}

pub(crate) fn emit_float_power(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
    ty: &Type,
) {
    let llvm_ty = llvm_type(ty).expect("validated float type");
    let intrinsic = match llvm_ty {
        "float" => "llvm.pow.f32",
        "double" => "llvm.pow.f64",
        _ => unreachable!("validated float power type"),
    };
    let _ = writeln!(
        text,
        "  %v{} = call {llvm_ty} @{intrinsic}({llvm_ty} %v{}, {llvm_ty} %v{})",
        destination.0, left.0, right.0
    );
}

pub(crate) fn emit_string_concat(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
) {
    let dest = destination.0;
    let _ = writeln!(text, "  %slenl{dest} = call i64 @strlen(ptr %v{})", left.0);
    let _ = writeln!(text, "  %slenr{dest} = call i64 @strlen(ptr %v{})", right.0);
    let _ = writeln!(text, "  %slens{dest} = add i64 %slenl{dest}, %slenr{dest}");
    let _ = writeln!(text, "  %sbytes{dest} = add i64 %slens{dest}, 1");
    let _ = writeln!(text, "  %v{dest} = call ptr @malloc(i64 %sbytes{dest})");
    let _ = writeln!(
        text,
        "  call void @llvm.memcpy.p0.p0.i64(ptr %v{dest}, ptr %v{}, i64 %slenl{dest}, i1 false)",
        left.0
    );
    let _ = writeln!(
        text,
        "  %stail{dest} = getelementptr i8, ptr %v{dest}, i64 %slenl{dest}"
    );
    let _ = writeln!(
        text,
        "  call void @llvm.memcpy.p0.p0.i64(ptr %stail{dest}, ptr %v{}, i64 %slenr{dest}, i1 false)",
        right.0
    );
    let _ = writeln!(
        text,
        "  %send{dest} = getelementptr i8, ptr %stail{dest}, i64 %slenr{dest}"
    );
    let _ = writeln!(text, "  store i8 0, ptr %send{dest}");
}

pub(crate) fn pow_intrinsic_declaration(ty: &Type) -> Option<&'static str> {
    match llvm_type(ty)? {
        "float" => Some("float @llvm.pow.f32(float, float)"),
        "double" => Some("double @llvm.pow.f64(double, double)"),
        "i8" | "i16" | "i32" | "i64" => {
            Some("{ i128, i1 } @llvm.smul.with.overflow.i128(i128, i128)")
        }
        _ => None,
    }
}

pub(crate) const STRING_CONCAT_DECLS: &str = "\
declare i64 @strlen(ptr)
declare ptr @malloc(i64)
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
";

fn emit_checked_i128_mul(
    text: &mut String,
    dest: u32,
    left: &str,
    right: &str,
    out: &str,
    from: &str,
) {
    let _ = writeln!(
        text,
        "  %{out}ov{dest} = call {{ i128, i1 }} @llvm.smul.with.overflow.i128(i128 %{left}{dest}, i128 %{right}{dest})"
    );
    let _ = writeln!(
        text,
        "  %{out}{dest} = extractvalue {{ i128, i1 }} %{out}ov{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %{out}f{dest} = extractvalue {{ i128, i1 }} %{out}ov{dest}, 1"
    );
    let ok = format!("{from}.ok");
    let _ = writeln!(
        text,
        "  br i1 %{out}f{dest}, label %trap_numeric_overflow, label %{ok}"
    );
    let _ = writeln!(text, "{ok}:");
}

fn emit_i128_range_trunc(
    text: &mut String,
    block_id: BlockId,
    dest: u32,
    llvm_ty: &str,
    ty: &Type,
    state: &mut EmissionState,
) {
    let (min, max) = i128_bounds(ty);
    let _ = writeln!(text, "  %shlo{dest} = icmp slt i128 %shraw{dest}, {min}");
    let _ = writeln!(text, "  %shhi{dest} = icmp sgt i128 %shraw{dest}, {max}");
    let _ = writeln!(text, "  %shov{dest} = or i1 %shlo{dest}, %shhi{dest}");
    let ok = take_continuation(block_id, state);
    let _ = writeln!(
        text,
        "  br i1 %shov{dest}, label %trap_numeric_overflow, label %{ok}"
    );
    let _ = writeln!(text, "{ok}:");
    let _ = writeln!(text, "  %v{dest} = trunc i128 %shraw{dest} to {llvm_ty}");
    state.needs_numeric_overflow_trap = true;
}

fn take_continuation(block_id: BlockId, state: &mut EmissionState) -> String {
    let name = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    name
}

fn emit_cast_integer(
    text: &mut String,
    result: &str,
    value: &str,
    from: &str,
    to: &str,
    ext: &str,
) {
    if from == to {
        let _ = writeln!(text, "  %{result} = add {from} 0, {value}");
    } else if llvm_int_width(from) < llvm_int_width(to) {
        let _ = writeln!(text, "  %{result} = {ext} {from} {value} to {to}");
    } else {
        let _ = writeln!(text, "  %{result} = trunc {from} {value} to {to}");
    }
}

fn llvm_int_width(llvm_ty: &str) -> u8 {
    match llvm_ty {
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        "i128" => 128,
        _ => unreachable!("integer LLVM type"),
    }
}

fn bit_width(ty: &Type) -> u32 {
    match integer_kind(ty) {
        IntegerType::Byte | IntegerType::Int8 => 8,
        IntegerType::Int16 | IntegerType::UInt16 => 16,
        IntegerType::Int32 | IntegerType::UInt32 => 32,
        IntegerType::Int64 | IntegerType::UInt64 => 64,
    }
}

fn extend_op(ty: &Type) -> &'static str {
    if is_unsigned(ty) { "zext" } else { "sext" }
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
