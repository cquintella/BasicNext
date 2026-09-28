// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Shared lowering helpers: integer kinds and ranges, coercions between LLVM
// types, literal rendering, symbol names, and unsupported-feature details.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::cast_possible_truncation
)]
use super::*;

pub(crate) fn integer_kind(ty: &Type) -> IntegerType {
    match ty {
        Type::Integer(kind) => *kind,
        Type::IntegerLiteral(_) => IntegerType::Int64,
        _ => IntegerType::Int32,
    }
}

/// Render an integer for LLVM IR. Never pass BN hex source (`0x…`) through:
/// LLVM treats `0x` as a floating-point constant encoding.
pub(crate) fn render_llvm_integer(value: i128, llvm_ty: &str) -> String {
    match llvm_ty {
        "i8" => render_signed_bits(value, 8),
        "i16" => render_signed_bits(value, 16),
        "i32" => render_signed_bits(value, 32),
        "i64" => render_signed_bits(value, 64),
        _ => value.to_string(),
    }
}

fn render_signed_bits(value: i128, bits: u32) -> String {
    let modulus = 1_i128 << bits;
    let normalized = value.rem_euclid(modulus);
    let sign = 1_i128 << (bits - 1);
    if normalized >= sign {
        (normalized - modulus).to_string()
    } else {
        normalized.to_string()
    }
}

pub(crate) fn coerce_integer(
    text: &mut String,
    value: ValueId,
    from_ty: &str,
    to_ty: &str,
    unsigned: bool,
) -> String {
    if from_ty == to_ty {
        return format!("%v{}", value.0);
    }
    let temp = format!("coer{}_{from_ty}_{to_ty}", value.0);
    let from_w = integer_llvm_width(from_ty);
    let to_w = integer_llvm_width(to_ty);
    if from_w < to_w {
        let opcode = if unsigned { "zext" } else { "sext" };
        let _ = writeln!(
            text,
            "  %{temp} = {opcode} {from_ty} %v{} to {to_ty}",
            value.0
        );
    } else {
        let _ = writeln!(text, "  %{temp} = trunc {from_ty} %v{} to {to_ty}", value.0);
    }
    format!("%{temp}")
}

pub(crate) fn coerce_to_type(text: &mut String, value: ValueId, from: &Type, to: &Type) -> String {
    let from_llvm = llvm_type(from).expect("validated coerce source");
    let to_llvm = llvm_type(to).expect("validated coerce target");
    if from_llvm == to_llvm {
        return format!("%v{}", value.0);
    }
    if from_llvm == "{ i1, ptr, i64 }"
        && matches!(to_llvm, "i8" | "i16" | "i32" | "i64" | "float" | "double")
    {
        let tag = format!("unionpayload{}", value.0);
        let _ = writeln!(
            text,
            "  %{tag} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            value.0
        );
        if matches!(to_llvm, "float" | "double") {
            let ftag = format!("{tag}f");
            let _ = writeln!(text, "  %{ftag} = bitcast i64 %{tag} to double");
            if to_llvm == "float" {
                let f32tag = format!("{tag}f32");
                let _ = writeln!(text, "  %{f32tag} = fptrunc double %{ftag} to float");
                return format!("%{f32tag}");
            }
            return format!("%{ftag}");
        }
        if to_llvm == "i64" {
            return format!("%{tag}");
        }
        let ctag = format!("{tag}c");
        let opcode = if integer_llvm_width(to_llvm) < 64 {
            "trunc"
        } else if is_unsigned(to) {
            "zext"
        } else {
            "sext"
        };
        let _ = writeln!(text, "  %{ctag} = {opcode} i64 %{tag} to {to_llvm}");
        return format!("%{ctag}");
    }
    if to_llvm == "{ i1, ptr, i64 }"
        && matches!(to, Type::Alternative(_))
        && (matches!(
            from_llvm,
            "i1" | "i8" | "i16" | "i32" | "i64" | "float" | "double"
        ) || *from == Type::String)
    {
        return wrap_in_alternative(text, value, from, from_llvm);
    }
    if from_llvm == "{ i1, ptr, i32 }" && to_llvm == "{ ptr, i32 }" {
        let tag = format!("endpointcoerce{}", value.0);
        let _ = writeln!(
            text,
            "  %{tag}_ptr = extractvalue {{ i1, ptr, i32 }} %v{}, 1",
            value.0
        );
        let _ = writeln!(
            text,
            "  %{tag}_port = extractvalue {{ i1, ptr, i32 }} %v{}, 2",
            value.0
        );
        let _ = writeln!(
            text,
            "  %{tag}_0 = insertvalue {{ ptr, i32 }} undef, ptr %{tag}_ptr, 0"
        );
        let _ = writeln!(
            text,
            "  %{tag} = insertvalue {{ ptr, i32 }} %{tag}_0, i32 %{tag}_port, 1"
        );
        return format!("%{tag}");
    }
    if matches!(from_llvm, "i8" | "i16" | "i32" | "i64")
        && matches!(to_llvm, "i8" | "i16" | "i32" | "i64")
    {
        return coerce_integer(
            text,
            value,
            from_llvm,
            to_llvm,
            is_unsigned(to) || is_unsigned(from),
        );
    }
    if matches!(
        (from_llvm, to_llvm),
        ("float", "double") | ("double", "float")
    ) {
        let temp = format!("fcoer{}_{from_llvm}_{to_llvm}", value.0);
        let opcode = if from_llvm == "float" {
            "fpext"
        } else {
            "fptrunc"
        };
        let _ = writeln!(
            text,
            "  %{temp} = {opcode} {from_llvm} %v{} to {to_llvm}",
            value.0
        );
        return format!("%{temp}");
    }
    format!("%v{}", value.0)
}

/// A scalar or STRING as the success of a `T OR Error` / `T OR EOF OR …`
/// value (`{ i1, ptr, i64 }`): a STRING in the pointer, any other scalar
/// in the `i64` payload (floats as their `double` bits), as `RETURN` builds
/// it. `LET x AS INTEGER OR Error = 3` stored the bare `i64` before.
fn wrap_in_alternative(text: &mut String, value: ValueId, from: &Type, from_llvm: &str) -> String {
    let v = value.0;
    let (pointer, payload) = match from_llvm {
        "ptr" => (format!("%v{v}"), "0".to_owned()),
        "i1" => {
            let _ = writeln!(text, "  %wrapbits{v} = zext i1 %v{v} to i64");
            ("null".to_owned(), format!("%wrapbits{v}"))
        }
        "float" | "double" => {
            let wide = if from_llvm == "float" {
                let _ = writeln!(text, "  %wrapwide{v} = fpext float %v{v} to double");
                format!("%wrapwide{v}")
            } else {
                format!("%v{v}")
            };
            let _ = writeln!(text, "  %wrapbits{v} = bitcast double {wide} to i64");
            ("null".to_owned(), format!("%wrapbits{v}"))
        }
        "i64" => ("null".to_owned(), format!("%v{v}")),
        narrow => {
            let extend = if is_unsigned(from) { "zext" } else { "sext" };
            let _ = writeln!(text, "  %wrapbits{v} = {extend} {narrow} %v{v} to i64");
            ("null".to_owned(), format!("%wrapbits{v}"))
        }
    };
    let _ = writeln!(
        text,
        "  %wraptag{v} = insertvalue {{ i1, ptr, i64 }} undef, i1 false, 0\n  %wrapptr{v} = insertvalue {{ i1, ptr, i64 }} %wraptag{v}, ptr {pointer}, 1\n  %wrap{v} = insertvalue {{ i1, ptr, i64 }} %wrapptr{v}, i64 {payload}, 2"
    );
    format!("%wrap{v}")
}

fn integer_llvm_width(llvm_ty: &str) -> u8 {
    match llvm_ty {
        "i1" => 1,
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        _ => 64,
    }
}

pub(crate) fn integer_range(kind: IntegerType) -> (i128, i128) {
    match kind {
        IntegerType::Byte => (0, i128::from(u8::MAX)),
        IntegerType::Int8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        IntegerType::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        IntegerType::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        IntegerType::Int64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        IntegerType::UInt16 => (0, i128::from(u16::MAX)),
        IntegerType::UInt32 => (0, i128::from(u32::MAX)),
        IntegerType::UInt64 => (0, i128::from(u64::MAX)),
    }
}

pub(crate) fn is_unsigned(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer(
            IntegerType::Byte | IntegerType::UInt16 | IntegerType::UInt32 | IntegerType::UInt64
        )
    )
}

pub(crate) fn extend_to_i64(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated extension type") {
        "i64" => format!("%v{}", value.0),
        llvm_ty => {
            let temp = format!("seedext{}", value.0);
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let _ = writeln!(text, "  %{temp} = {opcode} {llvm_ty} %v{} to i64", value.0);
            format!("%{temp}")
        }
    }
}

pub(crate) fn coerce_return_operand(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated return LLVM type") {
        "i32" => format!("%v{}", value.0),
        "i8" | "i16" => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let temp = format!("ret{}", value.0);
            let _ = writeln!(
                text,
                "  %{temp} = {opcode} {} %v{} to i32",
                llvm_type(ty).expect("validated integer return type"),
                value.0
            );
            format!("%{temp}")
        }
        "i64" => {
            let temp = format!("ret{}", value.0);
            let _ = writeln!(text, "  %{temp} = trunc i64 %v{} to i32", value.0);
            format!("%{temp}")
        }
        "{ i1, ptr, i64 }" => {
            let temp = format!("ret{}", value.0);
            let _ = writeln!(
                text,
                "  %{temp}flag = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
                value.0
            );
            let _ = writeln!(text, "  %{temp} = zext i1 %{temp}flag to i32");
            format!("%{temp}")
        }
        _ => unreachable!("validated return type"),
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn sanitize_symbol(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn static_global_name(class: &str, field: &str) -> String {
    format!(
        "@bn_st_{}_{}",
        sanitize_symbol(class),
        sanitize_symbol(field)
    )
}

pub(crate) fn class_init_flag(class: &str) -> String {
    format!("@bn_init_{}", sanitize_symbol(class))
}

pub(crate) fn parse_float_constant(value: &str) -> Option<f64> {
    match value {
        "NAN" | "nan" | "NaN" => Some(f64::NAN),
        "INF" | "inf" | "+INF" | "+inf" => Some(f64::INFINITY),
        "-INF" | "-inf" => Some(f64::NEG_INFINITY),
        _ => value.parse().ok(),
    }
}

pub(crate) fn render_float(value: f64, ty: &Type) -> String {
    let is_float32 = matches!(ty, Type::Float(FloatType::Float32));
    if value.is_nan() {
        return if is_float32 {
            "0x7FC00000".into()
        } else {
            "0x7FF8000000000000".into()
        };
    }
    if value == f64::INFINITY {
        return if is_float32 {
            "0x7F800000".into()
        } else {
            "0x7FF0000000000000".into()
        };
    }
    if value == f64::NEG_INFINITY {
        return if is_float32 {
            "0xFF800000".into()
        } else {
            "0xFFF0000000000000".into()
        };
    }
    let rendered = match ty {
        Type::Float(FloatType::Float32) => f64::from(value as f32).to_string(),
        _ => value.to_string(),
    };
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

pub(crate) fn unsupported_call_detail(module: &Module, name: &str) -> String {
    if let Some(provider) = provider_name(module, name) {
        return format!("{provider} calls");
    }
    if name.starts_with("HOST.") {
        return format!("unsupported HOST call {name}");
    }
    format!(
        "calls to user-defined function '{name}' are unavailable in the LLVM backend; \
         function calls are not supported by this build target yet (use 'bn run' \
         or inline the call)"
    )
}

pub(crate) fn provider_name(module: &Module, name: &str) -> Option<&'static str> {
    let module_id = name
        .strip_prefix('#')
        .and_then(|rest| rest.split('.').next())
        .and_then(|digits| digits.parse::<u32>().ok())?;
    if module
        .bnweb_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNWeb")
    } else if module
        .bndispatch_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNDispatch")
    } else if module
        .bnmath_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNMath")
    } else if module
        .bncrypto_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNCrypto")
    } else if module
        .bnjson_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNJson")
    } else {
        None
    }
}

/// True when `ty` carries a `BNJson.Json`, directly or inside an `OR Error`
/// alternative (a narrowed value keeps the aggregate, as `FS.File` does).
pub(crate) fn carries_bnjson(module: &Module, ty: &Type) -> bool {
    let named = |ty: &Type| {
        matches!(
            ty,
            Type::ImportedNamed { module: module_id, name }
                if name == "Json"
                    && module
                        .bnjson_providers
                        .contains(&bn_ir::ModuleId::from(*module_id))
        )
    };
    match ty {
        Type::Alternative(alternatives) => alternatives.iter().any(named),
        other => named(other),
    }
}

/// The `BNJson` member `name` selects, if this module imports `BNJson`.
pub(crate) fn bnjson_member<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    let method = name.rsplit('.').next()?;
    (!module.bnjson_providers.is_empty()
        && matches!(
            method,
            "Parse"
                | "Stringify"
                | "Object"
                | "Array"
                | "Kind"
                | "Has"
                | "Length"
                | "Clone"
                | "SetString"
                | "SetInteger"
                | "SetFloat"
                | "SetBoolean"
                | "SetNull"
                | "SetJson"
                | "GetString"
                | "GetInteger"
                | "GetFloat"
                | "GetBoolean"
                | "GetJson"
                | "AppendString"
                | "AppendInteger"
                | "AppendFloat"
                | "AppendBoolean"
                | "AppendNull"
                | "AppendJson"
                | "GetStringAt"
                | "GetIntegerAt"
                | "GetFloatAt"
                | "GetBooleanAt"
                | "GetJsonAt"
                | "SetStringAt"
                | "SetIntegerAt"
                | "SetFloatAt"
                | "SetBooleanAt"
                | "SetNullAt"
                | "SetJsonAt"
        ))
    .then_some(method)
}

/// True when `ty` is `BNCrypto.Bytes` from a module this program imports.
pub(crate) fn is_bncrypto_bytes_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "Bytes"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.Bytes`, either directly or inside an
/// `OR Error` alternative. A narrowed value keeps the aggregate type, exactly as
/// `FS.File` does, so both shapes reach method calls.
pub(crate) fn carries_bncrypto_bytes(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_bytes_type(module, alternative)),
        other => is_bncrypto_bytes_type(module, other),
    }
}

/// The `BNCrypto` member `name` selects, if this module imports `BNCrypto` and
/// the callee is part of the supported surface.
pub(crate) fn bncrypto_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    let method = name.rsplit('.').next()?;
    (!module.bncrypto_providers.is_empty()
        && matches!(
            method,
            "SHA256"
                | "SHA512"
                | "FromText"
                | "FromHex"
                | "Length"
                | "ToHex"
                | "SealAesGcm"
                | "OpenAesGcm"
                | "SealChaCha20"
                | "OpenChaCha20"
                | "HmacSha256"
                | "VerifyHmacSha256"
                | "Argon2id"
                | "Ed25519PublicKey"
                | "Ed25519Sign"
                | "Ed25519Verify"
                | "EcdsaP256PublicKey"
                | "EcdsaP256Sign"
                | "EcdsaP256Verify"
                | "Slice"
                | "MlKemKeypair"
                | "MlKemEncapsulate"
                | "MlKemDecapsulate"
                | "MlDsaKeypair"
                | "MlDsaSign"
                | "MlDsaVerify"
        ))
    .then_some(method)
}

pub(crate) fn unsupported_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    detail: &str,
) -> String {
    let span = instruction.span();
    let source_name = module.source_name.as_deref().unwrap_or("<unknown source>");
    let code = if detail.starts_with("unsupported HOST") {
        "TARGET_UNSUPPORTED_HOST"
    } else if matches!(
        instruction,
        Instruction::Vector { .. }
            | Instruction::Default { .. }
            | Instruction::Allocate { .. }
            | Instruction::Index { .. }
            | Instruction::SetIndex { .. }
    ) {
        "TARGET_UNSUPPORTED_TYPE"
    } else {
        "TARGET_UNSUPPORTED_OP"
    };
    format!(
        "{code}: {source_name}:{}:{}: {detail} in FUNCTION {}",
        span.start.line, span.start.column, function.name
    )
}

pub(crate) fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

pub(crate) fn input_runtime_ir() -> &'static str {
    r"
declare i32 @getchar()
declare ptr @realloc(ptr, i64)
declare void @free(ptr)

define ptr @bn_input(ptr %input.reusable) {
entry:
  %input.initial = call ptr @realloc(ptr %input.reusable, i64 64)
  br label %input.read
input.read:
  %input.buffer = phi ptr [ %input.initial, %entry ], [ %input.active.buffer, %input.store ], [ %input.buffer, %input.carriage ]
  %input.capacity = phi i64 [ 64, %entry ], [ %input.active.capacity, %input.store ], [ %input.capacity, %input.carriage ]
  %input.index = phi i64 [ 0, %entry ], [ %input.next, %input.store ], [ %input.index, %input.carriage ]
  %input.char = call i32 @getchar()
  %input.eof = icmp eq i32 %input.char, -1
  br i1 %input.eof, label %input.eof.check, label %input.line.check
input.eof.check:
  %input.empty = icmp eq i64 %input.index, 0
  br i1 %input.empty, label %input.eof.out, label %input.done
input.line.check:
  %input.newline = icmp eq i32 %input.char, 10
  %input.cr = icmp eq i32 %input.char, 13
  br i1 %input.newline, label %input.done, label %input.cr.check
input.cr.check:
  br i1 %input.cr, label %input.carriage, label %input.store.check
input.carriage:
  br label %input.read
input.store.check:
  %input.required = add i64 %input.index, 2
  %input.must.grow = icmp ugt i64 %input.required, %input.capacity
  br i1 %input.must.grow, label %input.grow, label %input.keep
input.grow:
  %input.new.capacity = shl i64 %input.capacity, 1
  %input.grown = call ptr @realloc(ptr %input.buffer, i64 %input.new.capacity)
  br label %input.store
input.keep:
  br label %input.store
input.store:
  %input.active.buffer = phi ptr [ %input.grown, %input.grow ], [ %input.buffer, %input.keep ]
  %input.active.capacity = phi i64 [ %input.new.capacity, %input.grow ], [ %input.capacity, %input.keep ]
  %input.byte = trunc i32 %input.char to i8
  %input.slot = getelementptr i8, ptr %input.active.buffer, i64 %input.index
  store i8 %input.byte, ptr %input.slot
  %input.next = add i64 %input.index, 1
  br label %input.read
input.done:
  %input.end = getelementptr i8, ptr %input.buffer, i64 %input.index
  store i8 0, ptr %input.end
  ret ptr %input.buffer
input.eof.out:
  call void @free(ptr %input.buffer)
  ret ptr @.bn_eof
}
"
}

pub(crate) fn string_byte_length_ir() -> &'static str {
    r"
define i64 @bn_string_byte_length(ptr %text) {
entry:
  br label %length.scan
length.scan:
  %length.index = phi i64 [ 0, %entry ], [ %length.next, %length.more ]
  %length.slot = getelementptr i8, ptr %text, i64 %length.index
  %length.byte = load i8, ptr %length.slot
  %length.done = icmp eq i8 %length.byte, 0
  br i1 %length.done, label %length.out, label %length.more
length.more:
  %length.next = add i64 %length.index, 1
  br label %length.scan
length.out:
  ret i64 %length.index
}
"
}

pub(crate) fn is_canonical_timezone(text: &str) -> bool {
    if text == "UTC" {
        return true;
    }
    let mut parts = 0;
    for part in text.split('/') {
        parts += 1;
        let mut characters = part.chars();
        let Some(first) = characters.next() else {
            return false;
        };
        if !first.is_ascii_alphabetic()
            || !characters.all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '+')
            })
        {
            return false;
        }
    }
    parts >= 2
}

pub(crate) fn escape_llvm(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b' '..=b'!' | b'#'..=b'[' | b']'..=b'~' => (byte as char).to_string(),
            _ => format!("\\{byte:02X}"),
        })
        .collect()
}

pub(crate) fn instruction_name(instruction: &Instruction) -> &'static str {
    match instruction {
        Instruction::Constant { .. } => "constants",
        Instruction::Default { .. } => "defaults",
        Instruction::Phi { .. } => "Phi merges",
        Instruction::Load { .. } => "loads",
        Instruction::Store { .. } => "stores",
        Instruction::Copy { .. } => "copies",
        Instruction::Unary { .. } => "unary operations",
        Instruction::Binary { .. } => "binary operations",
        Instruction::Cast { .. } => "casts",
        Instruction::Call { .. } => "calls",
        Instruction::DispatchSubmit { .. } | Instruction::DispatchAwait { .. } => {
            "asynchronous dispatch"
        }
        Instruction::Input { .. } => "INPUT",
        Instruction::Vector { .. } => "vectors",
        Instruction::Index { .. } => "indexing",
        Instruction::Member { .. } => "member access",
        Instruction::SetIndex { .. } => "indexed stores",
        Instruction::SetMemberIndex { .. } => "indexed member stores",
        Instruction::SetFieldIndex { .. } => "indexed field stores",
        Instruction::SetStaticIndex { .. } => "indexed static stores",
        Instruction::Length { .. } => "LEN",
        Instruction::SizeOf { .. } => "SIZEOF",
        Instruction::Print { .. } => "PRINT",
        Instruction::ClearScreen { .. } | Instruction::Beep { .. } => "console operations",
        Instruction::Allocate { .. } => "allocation",
        Instruction::Release { .. } => "release",
        Instruction::SetMember { .. } => "member stores",
        Instruction::SetField { .. } => "field stores",
        Instruction::EnsureClass { .. } => "class initialization",
        Instruction::LoadStatic { .. } => "static loads",
        Instruction::StoreStatic { .. } => "static stores",
    }
}

pub(crate) fn unsupported_instruction_detail(instruction: &Instruction) -> String {
    match instruction {
        Instruction::Phi { ty, .. } => format!(
            "LLVM lowering for Phi with type '{}' is unavailable",
            crate::display_type(ty)
        ),
        Instruction::Vector { ty, .. } => format!(
            "LLVM lowering for vector type '{}' is unavailable",
            crate::display_type(ty)
        ),
        Instruction::Allocate { type_name, ty, .. } => format!(
            "LLVM lowering for allocation of '{type_name}' as '{}' is unavailable",
            crate::display_type(ty)
        ),
        Instruction::Index { ty, .. }
        | Instruction::SetIndex { ty, .. }
        | Instruction::SetMemberIndex { ty, .. } => format!(
            "LLVM lowering for indexed access producing '{}' is unavailable",
            crate::display_type(ty)
        ),
        Instruction::SetFieldIndex { ty, .. } => format!(
            "LLVM lowering for indexed field assignment of '{}' is unavailable",
            crate::display_type(ty)
        ),
        Instruction::SetStaticIndex { ty, .. } => format!(
            "LLVM lowering for indexed static assignment of '{}' is unavailable",
            crate::display_type(ty)
        ),
        Instruction::Default {
            ty,
            dimensions,
            dynamic_dimensions,
            ..
        } if !dimensions.is_empty() || !dynamic_dimensions.is_empty() => format!(
            "LLVM lowering for default value of '{}' with dimensions is unavailable",
            crate::display_type(ty)
        ),
        Instruction::Release { .. } => "LLVM lowering for pointer release is unavailable".into(),
        _ => instruction_name(instruction).into(),
    }
}
