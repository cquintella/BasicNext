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
use crate::ir::InstSink as _;
use crate::ir::LlvmType;
#[path = "library_types.rs"]
mod library_types;
pub(crate) use library_types::*;
#[path = "input_runtime.rs"]
mod input_runtime;

pub(crate) fn integer_kind(ty: &Type) -> IntegerType {
    match ty {
        Type::Integer(kind) => *kind,
        Type::IntegerLiteral(_) => IntegerType::Int64,
        _ => IntegerType::Int32,
    }
}

/// The constant of an integer `value` in `llvm_ty`: its bit pattern read as a
/// signed number, as LLVM reads it. Never BN hex source (`0x…`), which LLVM
/// takes for a floating-point encoding.
pub(crate) fn integer_constant(
    value: i128,
    llvm_ty: &crate::ir::LlvmType,
) -> crate::ir::LlvmOperand {
    match llvm_ty.integer_bits() {
        Some(bits @ (8 | 16 | 32 | 64)) => crate::ir::LlvmOperand::int128(signed_bits(value, bits)),
        _ => crate::ir::LlvmOperand::int128(value),
    }
}

fn signed_bits(value: i128, bits: u32) -> i128 {
    let modulus = 1_i128 << bits;
    let normalized = value.rem_euclid(modulus);
    let sign = 1_i128 << (bits - 1);
    if normalized >= sign {
        normalized - modulus
    } else {
        normalized
    }
}

pub(crate) fn coerce_integer(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    from_ty: &crate::ir::LlvmType,
    to_ty: &crate::ir::LlvmType,
    unsigned: bool,
) -> crate::ir::LlvmOperand {
    if from_ty == to_ty {
        return crate::ir::LlvmOperand::reg(format!("v{}", value.0));
    }
    let temp = format!("coer{}_{}_{from_ty}_{to_ty}", value.0, text.fresh_tag());
    let from_w = from_ty.integer_bits().unwrap_or(64);
    let to_w = to_ty.integer_bits().unwrap_or(64);
    let own = crate::ir::LlvmOperand::reg(format!("v{}", value.0));
    let (from, to) = (from_ty.clone(), to_ty.clone());
    if from_w < to_w {
        let op = if unsigned {
            crate::ir::CastOp::ZExt
        } else {
            crate::ir::CastOp::SExt
        };
        text.assign(temp.clone(), crate::ir::LlvmInst::cast(op, from, own, to));
    } else {
        text.assign(
            temp.clone(),
            crate::ir::LlvmInst::cast(crate::ir::CastOp::Trunc, from, own, to),
        );
    }
    crate::ir::LlvmOperand::reg(&temp)
}

pub(crate) fn region_element_label(element: &Type) -> String {
    match element {
        Type::Integer(kind) => format!("{kind:?}"),
        Type::Float(kind) => format!("{kind:?}"),
        _ => "POINTER".into(),
    }
}

pub(crate) fn is_integer_handle_type(ty: &Type) -> bool {
    match ty {
        Type::Named(name) | Type::TypeName(name) => {
            let base = name.rsplit('.').next().unwrap_or(name);
            matches!(
                base,
                "File"
                    | "TCPStream"
                    | "TCPListener"
                    | "UDPSocket"
                    | "UDPPacket"
                    | "Connection"
                    | "DataFrame"
                    | "Bytes"
                    | "Queue"
                    | "Ticket"
                    | "Group"
                    | "Barrier"
                    | "Semaphore"
                    | "Mutex"
                    | "SessionStore"
                    | "CookieJar"
                    | "Scraper"
                    | "ACL"
                    | "Client"
                    | "Server"
                    | "EgressPolicy"
                    | "TLSConfig"
                    | "Response"
                    | "Request"
                    | "Json"
                    | "Logger"
                    | "Result"
            )
        }
        Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } => {
            matches!(
                name.as_str(),
                "File"
                    | "TCPStream"
                    | "TCPListener"
                    | "UDPSocket"
                    | "UDPPacket"
                    | "Connection"
                    | "DataFrame"
                    | "Bytes"
                    | "Queue"
                    | "Ticket"
                    | "Group"
                    | "Barrier"
                    | "Semaphore"
                    | "Mutex"
                    | "SessionStore"
                    | "CookieJar"
                    | "Scraper"
                    | "ACL"
                    | "Client"
                    | "Server"
                    | "EgressPolicy"
                    | "TLSConfig"
                    | "Response"
                    | "Request"
                    | "Json"
                    | "Logger"
                    | "Result"
            )
        }
        Type::Alternative(members) => members.iter().any(is_integer_handle_type),
        _ => false,
    }
}

pub(crate) fn coerce_to_type(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    from: &Type,
    to: &Type,
) -> crate::ir::LlvmOperand {
    let from_llvm = llvm_type(from).expect("validated coerce source");
    let to_llvm = llvm_type(to).expect("validated coerce target");
    if from_llvm == to_llvm {
        return crate::ir::LlvmOperand::reg(format!("v{}", value.0));
    }
    if from_llvm == crate::layout::handle_result_ty() && to_llvm == LlvmType::Ptr {
        let union_ty = LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr, LlvmType::I64]);
        if is_integer_handle_type(from) || is_integer_handle_type(to) {
            let tag = format!("unhdlpayload{}_{}", value.0, text.fresh_tag());
            text.assign(
                tag.clone(),
                crate::ir::LlvmInst::extract(
                    union_ty,
                    crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                    2,
                ),
            );
            let ptag = format!("unhdlptr{}_{}", value.0, text.fresh_tag());
            text.assign(
                ptag.clone(),
                crate::ir::LlvmInst::cast(
                    crate::ir::CastOp::IntToPtr,
                    LlvmType::I64,
                    crate::ir::LlvmOperand::reg(tag),
                    LlvmType::Ptr,
                ),
            );
            return crate::ir::LlvmOperand::reg(&ptag);
        }
        let tag = format!("unptrpayload{}_{}", value.0, text.fresh_tag());
        text.assign(
            tag.clone(),
            crate::ir::LlvmInst::extract(
                union_ty,
                crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                1,
            ),
        );
        return crate::ir::LlvmOperand::reg(&tag);
    }
    if from_llvm == crate::layout::handle_result_ty()
        && matches!(
            to_llvm,
            LlvmType::I8
                | LlvmType::I16
                | LlvmType::I32
                | LlvmType::I64
                | LlvmType::Float
                | LlvmType::Double
        )
    {
        let tag = format!("unionpayload{}_{}", value.0, text.fresh_tag());
        let union_ty = LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr, LlvmType::I64]);
        text.assign(
            tag.clone(),
            crate::ir::LlvmInst::extract(
                union_ty,
                crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                2,
            ),
        );
        if matches!(to_llvm, LlvmType::Float | LlvmType::Double) {
            let ftag = format!("{tag}f");
            text.assign(
                ftag.clone(),
                crate::ir::LlvmInst::cast(
                    crate::ir::CastOp::BitCast,
                    LlvmType::I64,
                    crate::ir::LlvmOperand::reg(&tag),
                    LlvmType::Double,
                ),
            );
            if to_llvm == LlvmType::Float {
                let f32tag = format!("{tag}f32");
                text.assign(
                    f32tag.clone(),
                    crate::ir::LlvmInst::cast(
                        crate::ir::CastOp::FPTrunc,
                        LlvmType::Double,
                        crate::ir::LlvmOperand::reg(ftag),
                        LlvmType::Float,
                    ),
                );
                return crate::ir::LlvmOperand::reg(&f32tag);
            }
            return crate::ir::LlvmOperand::reg(&ftag);
        }
        if to_llvm == LlvmType::I64 {
            return crate::ir::LlvmOperand::reg(&tag);
        }
        let ctag = format!("{tag}c");
        let opcode = if to_llvm.integer_bits().unwrap_or(64) < 64 {
            crate::ir::CastOp::Trunc
        } else if is_unsigned(to) {
            crate::ir::CastOp::ZExt
        } else {
            crate::ir::CastOp::SExt
        };
        text.assign(
            ctag.clone(),
            crate::ir::LlvmInst::cast(
                opcode,
                LlvmType::I64,
                crate::ir::LlvmOperand::reg(tag),
                to_llvm,
            ),
        );
        return crate::ir::LlvmOperand::reg(&ctag);
    }
    if let Some(constructed) = alternatives::construct(text, value, from, to) {
        return constructed;
    }
    if from_llvm == crate::layout::status_endpoint_ty() && to_llvm == crate::layout::vector_ty() {
        let tag = format!("endpointcoerce{}_{}", value.0, text.fresh_tag());
        let ep_in = LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr, LlvmType::I32]);
        let ep_out = LlvmType::struct_of([LlvmType::Ptr, LlvmType::I32]);
        let r = |suffix: &str| crate::ir::LlvmOperand::reg(format!("{tag}_{suffix}"));
        text.assign(
            format!("{tag}_ptr"),
            crate::ir::LlvmInst::extract(
                ep_in.clone(),
                crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                1,
            ),
        );
        text.assign(
            format!("{tag}_port"),
            crate::ir::LlvmInst::extract(
                ep_in,
                crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                2,
            ),
        );
        text.assign(
            format!("{tag}_0"),
            crate::ir::LlvmInst::insert(
                ep_out.clone(),
                crate::ir::LlvmOperand::undef(),
                LlvmType::Ptr,
                r("ptr"),
                0,
            ),
        );
        text.assign(
            tag.clone(),
            crate::ir::LlvmInst::insert(ep_out, r("0"), LlvmType::I32, r("port"), 1),
        );
        return crate::ir::LlvmOperand::reg(&tag);
    }
    if from_llvm == crate::layout::vector_ty() && to_llvm == crate::layout::region_ty() {
        let elem_code = match from {
            Type::Pointer { element, .. } => {
                bn_types::alternatives::member_code(element).unwrap_or(0)
            }
            _ => 0,
        };
        let tag = format!("voidcoer{}_{}", value.0, text.fresh_tag());
        let fat_void = crate::layout::region_ty();
        let val = crate::ir::LlvmOperand::reg(format!("v{}", value.0));
        let ptr_val = format!("{tag}_ptr");
        let len_val = format!("{tag}_len");
        text.assign(
            ptr_val.clone(),
            crate::ir::LlvmInst::extract(crate::layout::vector_ty(), val.clone(), 0),
        );
        text.assign(
            len_val.clone(),
            crate::ir::LlvmInst::extract(crate::layout::vector_ty(), val, 1),
        );
        let ins0 = format!("{tag}_0");
        text.assign(
            ins0.clone(),
            crate::ir::LlvmInst::insert(
                fat_void.clone(),
                crate::ir::LlvmOperand::undef(),
                LlvmType::Ptr,
                crate::ir::LlvmOperand::reg(ptr_val),
                0,
            ),
        );
        let ins1 = format!("{tag}_1");
        text.assign(
            ins1.clone(),
            crate::ir::LlvmInst::insert(
                fat_void.clone(),
                crate::ir::LlvmOperand::reg(ins0),
                LlvmType::I32,
                crate::ir::LlvmOperand::reg(len_val),
                1,
            ),
        );
        text.assign(
            tag.clone(),
            crate::ir::LlvmInst::insert(
                fat_void,
                crate::ir::LlvmOperand::reg(ins1),
                LlvmType::I32,
                crate::ir::LlvmOperand::int(i64::from(elem_code)),
                2,
            ),
        );
        return crate::ir::LlvmOperand::reg(&tag);
    }
    if from_llvm.is_integer() && to_llvm.is_integer() {
        return coerce_integer(
            text,
            value,
            &from_llvm,
            &to_llvm,
            is_unsigned(to) || is_unsigned(from),
        );
    }
    if matches!(
        (from_llvm.clone(), to_llvm.clone()),
        (LlvmType::Float, LlvmType::Double) | (LlvmType::Double, LlvmType::Float)
    ) {
        let temp = format!(
            "fcoer{}_{}_{from_llvm}_{to_llvm}",
            value.0,
            text.fresh_tag()
        );
        let (opcode, from, to) = if from_llvm == LlvmType::Float {
            (crate::ir::CastOp::FPExt, LlvmType::Float, LlvmType::Double)
        } else {
            (
                crate::ir::CastOp::FPTrunc,
                LlvmType::Double,
                LlvmType::Float,
            )
        };
        text.assign(
            temp.clone(),
            crate::ir::LlvmInst::cast(
                opcode,
                from,
                crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                to,
            ),
        );
        return crate::ir::LlvmOperand::reg(&temp);
    }
    crate::ir::LlvmOperand::reg(format!("v{}", value.0))
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

pub(crate) fn extend_to_i64(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> crate::ir::LlvmOperand {
    match llvm_type(ty).expect("validated extension type") {
        LlvmType::I64 => crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
        llvm_ty => {
            let temp = format!("seedext{}_{}", value.0, text.fresh_tag());
            let opcode = if is_unsigned(ty) {
                crate::ir::CastOp::ZExt
            } else {
                crate::ir::CastOp::SExt
            };
            text.assign(
                temp.clone(),
                crate::ir::LlvmInst::cast(
                    opcode,
                    llvm_ty,
                    crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                    LlvmType::I64,
                ),
            );
            crate::ir::LlvmOperand::reg(&temp)
        }
    }
}

pub(crate) fn coerce_return_operand(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> crate::ir::LlvmOperand {
    match llvm_type(ty).expect("validated return LLVM type") {
        LlvmType::I32 => crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
        LlvmType::I8 | LlvmType::I16 => {
            let opcode = if is_unsigned(ty) {
                crate::ir::CastOp::ZExt
            } else {
                crate::ir::CastOp::SExt
            };
            let temp = format!("ret{}_{}", value.0, text.fresh_tag());
            let llvm_ty = llvm_type(ty).expect("validated integer return type");
            text.assign(
                temp.clone(),
                crate::ir::LlvmInst::cast(
                    opcode,
                    llvm_ty,
                    crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                    LlvmType::I32,
                ),
            );
            crate::ir::LlvmOperand::reg(&temp)
        }
        LlvmType::I64 => {
            let temp = format!("ret{}_{}", value.0, text.fresh_tag());
            text.assign(
                temp.clone(),
                crate::ir::LlvmInst::cast(
                    crate::ir::CastOp::Trunc,
                    LlvmType::I64,
                    crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                    LlvmType::I32,
                ),
            );
            crate::ir::LlvmOperand::reg(&temp)
        }
        layout if layout == crate::layout::handle_result_ty() => {
            let temp = format!("ret{}_{}", value.0, text.fresh_tag());
            let union_ty = LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr, LlvmType::I64]);
            text.assign(
                format!("{temp}flag"),
                crate::ir::LlvmInst::extract(
                    union_ty,
                    crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
                    0,
                ),
            );
            text.assign(
                temp.clone(),
                crate::ir::LlvmInst::cast(
                    crate::ir::CastOp::ZExt,
                    LlvmType::I1,
                    crate::ir::LlvmOperand::reg(format!("{temp}flag")),
                    LlvmType::I32,
                ),
            );
            crate::ir::LlvmOperand::reg(&temp)
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

/// LLVM reads a hexadecimal `float` constant in the 64-bit `double` encoding
/// (`LangRef`, "Simple Constants"), so the special values have one spelling for
/// both widths; a 32-bit pattern is rejected by clang (known issue K34).
/// The constant of a floating-point `value` of type `ty`; a `FLOAT32` value is
/// rounded to single precision first, as LLVM requires of a `float` constant.
/// The identity of floating-point addition, `-0.0`: `-0.0 + x` is `x` for
/// every `x`, both zeros included, while `+0.0 + -0.0` is `+0.0`. A float
/// constant or copy written as `fadd` with it, and a negation written as
/// `fsub` from it, keep the sign of zero (math.md, "Numeric model").
pub(crate) fn float_additive_identity() -> crate::ir::LlvmOperand {
    crate::ir::LlvmOperand::float(-0.0)
}

pub(crate) fn float_constant(value: f64, ty: &Type) -> crate::ir::LlvmOperand {
    let value = match ty {
        Type::Float(FloatType::Float32) => f64::from(value as f32),
        _ => value,
    };
    crate::ir::LlvmOperand::float(value)
}

pub(crate) fn unsupported_call_detail(module: &Module, name: &str) -> String {
    if let Some(provider) = provider_name(module, name) {
        return format!("{provider} calls");
    }
    if name.starts_with("HOST.") {
        return format!("unsupported HOST call {name}");
    }
    bn_catalog::fill("llvm", "call.user_function.unsupported", &[("name", &name)])
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
        .bncompress_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some(bn_catalog::text("modules", "module.compress"))
    } else if module
        .bnjson_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNJson")
    } else if module
        .bnsqlite_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some(bn_catalog::text("llvm", "msg.bnsqlite"))
    } else {
        None
    }
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
    bn_types::literals::parse_integer(value)
}

/// Declares the C functions `bn_input` calls and defines it.
pub(crate) fn emit_input_runtime(text: &mut crate::ir::LlvmModule) {
    text.blank();
    crate::ir::write_declarations(text, crate::runtime_abi::INPUT_DECLARATIONS);
    text.blank();
    text.function(input_runtime::input_function());
    text.function(input_runtime::input_str_function());
}

pub(crate) fn instruction_name(instruction: &Instruction) -> &'static str {
    match instruction {
        Instruction::Constant { .. } => "constants",
        Instruction::Default { .. } => "defaults",
        Instruction::Phi { .. } => "Phi merges",
        Instruction::Load { .. } => "loads",
        Instruction::Retain { .. }
        | Instruction::Take { .. }
        | Instruction::TakeMember { .. }
        | Instruction::EndBinding { .. } => "explicit ownership operations",
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

#[cfg(test)]
mod tests {
    use super::integer_constant;
    use crate::ir::LlvmType;

    #[test]
    fn integer_constants_keep_the_signed_bit_pattern_without_a_cast_wrap() {
        assert_eq!(integer_constant(255, &LlvmType::I8).to_string(), "-1");
        assert_eq!(integer_constant(-129, &LlvmType::I8).to_string(), "127");
        assert_eq!(integer_constant(65_536, &LlvmType::I16).to_string(), "0");
        assert_eq!(
            integer_constant(-9_223_372_036_854_775_809, &LlvmType::I64).to_string(),
            "9223372036854775807"
        );
    }
}
