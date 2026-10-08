// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// User function signatures, parameter spills, direct calls with argument
// coercion, and virtual method dispatch by runtime class name.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::{
    ir::{
        CastOp, ICmpCond, InstSink, LlvmFunction, LlvmInst, LlvmOperand,
        LlvmType::{self, I1, I32, I64, Ptr, Void},
    },
    layout::{handle_result_ty, typed_llvm, vector_ty},
};
use runtime_abi::STR_EQ;

/// A call argument: its LLVM type and operand.
type Argument = (LlvmType, LlvmOperand);

pub(crate) fn emit_user_signature(
    text: &mut String,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
) {
    let ret = function_return_llvm(&function.return_type).expect("validated return type");
    let params = (0..function.parameters.len())
        .zip(&function.parameters)
        .map(|(index, symbol)| {
            (
                typed_llvm(parameter_type(function, *symbol, analysis)),
                format!("p{index}"),
            )
        })
        .collect();
    let signature = LlvmFunction::new_definition(
        llvm_function_symbol(&function.name),
        typed_llvm(ret),
        params,
    );
    let _ = writeln!(text, "\n{}", signature.header());
}

pub(crate) fn store_parameters(
    text: &mut String,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
    symbol_names: &HashMap<SymbolId, usize>,
) {
    for (index, symbol) in function.parameters.iter().enumerate() {
        let Some(&slot) = symbol_names.get(symbol) else {
            continue;
        };
        let incoming = format!("%p{index}");
        // A vector argument is a copy: the callee owns its elements.
        let storage = format!("%vstore{slot}");
        let value = (analysis.symbols.get(symbol))
            .and_then(|ty| vectors::emit_vector_copy(text, &storage, &incoming, ty));
        text.emit(LlvmInst::store(
            typed_llvm(parameter_type(function, *symbol, analysis)),
            LlvmOperand::raw(value.unwrap_or(incoming)),
            LlvmOperand::reg(format!("s{slot}")),
        ));
    }
}

/// The LLVM type of a parameter: its symbol's type, else the type of a
/// load or store of it, else `ptr` (object and `SELF` parameters may be
/// unused in the body, e.g. `Animal.Speak`).
fn parameter_type<'a>(
    function: &'a Function,
    symbol: SymbolId,
    analysis: &'a LoweringAnalysis<'a>,
) -> &'static str {
    analysis
        .symbols
        .get(&symbol)
        .and_then(llvm_type)
        .or_else(|| parameter_semantic_type(function, symbol).and_then(llvm_type))
        .unwrap_or("ptr")
}

fn parameter_semantic_type(function: &Function, symbol: SymbolId) -> Option<&Type> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::Load {
                symbol: loaded, ty, ..
            } if *loaded == symbol => Some(ty),
            Instruction::Store {
                symbol: stored, ty, ..
            } if *stored == symbol => Some(ty),
            _ => None,
        })
}

/// The operand passed for `argument` to a parameter of LLVM type
/// `param_llvm`, converting the argument's representation where they differ.
#[allow(clippy::too_many_arguments)]
fn call_operand(
    text: &mut String,
    destination: ValueId,
    argument: ValueId,
    arg_ty: &Type,
    param_ty: &Type,
    param_llvm: &str,
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) -> LlvmOperand {
    let reg = LlvmOperand::reg;
    let arg_llvm = llvm_type(arg_ty);
    let value = value_reg(argument);
    // A union result carries the payload in field 2 and a message in field 1.
    let union_payload = |text: &mut String, tag: &str| {
        text.assign(
            format!("{tag}raw"),
            LlvmInst::extract(handle_result_ty(), value_reg(argument), 2),
        );
        reg(format!("{tag}raw"))
    };
    if param_llvm == "{ ptr, i32 }" && arg_llvm == Some("{ i1, ptr, i32 }") {
        let tag = state.continuation_count;
        state.continuation_count += 1;
        let endpoint = LlvmType::struct_of([I1, Ptr, I32]);
        text.assign(
            format!("call_endpoint_ptr{tag}"),
            LlvmInst::extract(endpoint.clone(), value.clone(), 1),
        );
        text.assign(
            format!("call_endpoint_port{tag}"),
            LlvmInst::extract(endpoint, value, 2),
        );
        text.assign(
            format!("call_endpoint{tag}_0"),
            LlvmInst::insert(
                vector_ty(),
                LlvmOperand::undef(),
                Ptr,
                reg(format!("call_endpoint_ptr{tag}")),
                0,
            ),
        );
        text.assign(
            format!("call_endpoint{tag}"),
            LlvmInst::insert(
                vector_ty(),
                reg(format!("call_endpoint{tag}_0")),
                I32,
                reg(format!("call_endpoint_port{tag}")),
                1,
            ),
        );
        return reg(format!("call_endpoint{tag}"));
    }
    if param_llvm == "i1" {
        return LlvmOperand::raw(i1_operand(text, analysis, state, argument));
    }
    if param_llvm == "ptr" && arg_llvm == Some("ptr") {
        return value;
    }
    if param_llvm == "ptr" && arg_llvm == Some("{ i1, ptr, i64 }") {
        let tag = format!("callstr{}_{}", destination.0, argument.0);
        text.assign(
            format!("{tag}msg"),
            LlvmInst::extract(handle_result_ty(), value, 1),
        );
        let raw = union_payload(text, &tag);
        text.assign(
            format!("{tag}payload"),
            LlvmInst::cast(CastOp::IntToPtr, I64, raw, Ptr),
        );
        text.assign(
            format!("{tag}hasmsg"),
            LlvmInst::icmp(
                ICmpCond::Ne,
                Ptr,
                reg(format!("{tag}msg")),
                LlvmOperand::null(),
            ),
        );
        text.assign(
            tag.clone(),
            LlvmInst::select(
                reg(format!("{tag}hasmsg")),
                Ptr,
                reg(format!("{tag}msg")),
                reg(format!("{tag}payload")),
            ),
        );
        return reg(tag);
    }
    LlvmOperand::raw(coerce_to_type(text, argument, arg_ty, param_ty))
}

pub(crate) fn lower_user_call(
    text: &mut String,
    module: &Module,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let is_super = name.starts_with("@super:");
    let resolved = name.strip_prefix("@super:").unwrap_or(name);
    let callee = module
        .functions
        .iter()
        .find(|function| function.name == resolved)
        .unwrap_or_else(|| panic!("validated user function {resolved}"));
    let ret =
        typed_llvm(function_return_llvm(&callee.return_type).expect("validated user return type"));
    let mut args = Vec::with_capacity(arguments.len());
    for (index, (argument, param_symbol)) in arguments.iter().zip(&callee.parameters).enumerate() {
        let arg_ty = analysis
            .values
            .get(argument)
            .expect("validated call argument type");
        // The type the signature declares (`emit_user_signature`): a parameter
        // the callee never reads takes its declared type.
        let declared = super::analysis::declared_parameter(module, resolved, index);
        let param_ty = parameter_semantic_type(callee, *param_symbol)
            .or(declared.as_ref())
            .unwrap_or(arg_ty);
        if llvm_type(param_ty) == Some("{ i1, double }")
            || (llvm_type(arg_ty) == Some("{ i1, double }")
                && matches!(param_ty, Type::Float(_) | Type::FloatLiteral))
        {
            let temp = format!("callopt{}", argument.0);
            text.assign(
                temp.clone(),
                LlvmInst::extract(
                    LlvmType::struct_of([I1, LlvmType::Double]),
                    value_reg(*argument),
                    1,
                ),
            );
            args.push((LlvmType::Double, LlvmOperand::reg(temp)));
            continue;
        }
        let param_llvm = llvm_type(param_ty).unwrap_or("ptr");
        let operand = call_operand(
            text,
            destination,
            *argument,
            arg_ty,
            param_ty,
            param_llvm,
            analysis,
            state,
        );
        args.push((typed_llvm(param_llvm), operand));
    }
    let method = resolved.rsplit('.').next().unwrap_or(resolved);
    let virtualish = !is_super
        && !matches!(
            module.kind_of(resolved),
            Some(FunctionKind::Constructor | FunctionKind::FieldInit | FunctionKind::Init)
        )
        && !arguments.is_empty()
        && analysis
            .values
            .get(&arguments[0])
            .is_some_and(|ty| llvm_type(ty) == Some("ptr"));
    if virtualish {
        let static_class = resolved.rsplit_once('.').map_or("", |(c, _)| c);
        let mut overrides: Vec<(&str, &str)> = Vec::new();
        for candidate_class in module.field_layouts.keys() {
            if (candidate_class == static_class
                || module.class_model.is_upcast(candidate_class, static_class))
                && let Some(target) =
                    bn_ir::dispatch::resolve_method(module, candidate_class, method)
                && target != resolved
            {
                overrides.push((candidate_class.as_str(), target));
            }
        }
        if !overrides.is_empty() {
            emit_virtual_method_call(
                text,
                state,
                destination,
                resolved,
                &overrides,
                &args,
                &ret,
                arguments[0],
            );
            return;
        }
    }
    let call = LlvmInst::call(ret.clone(), &llvm_function_symbol(resolved), args);
    if ret == Void {
        text.emit(call);
    } else {
        // A vector result is copied out of the callee's buffer at once.
        let relocated = vectors::returned_vector(&callee.return_type).is_some();
        let raw = format!("v{}{}", if relocated { "raw" } else { "" }, destination.0);
        text.assign(raw.clone(), call);
        if relocated {
            let (buffer, out) = (
                format!("%vret{}", destination.0),
                format!("%v{}", destination.0),
            );
            vectors::emit_vector_relocate(
                text,
                &format!("%{raw}"),
                &callee.return_type,
                &buffer,
                &out,
            );
        }
        if analysis.owned_struct_results.contains(&destination) {
            text.emit(LlvmInst::store(
                Ptr,
                value_reg(destination),
                LlvmOperand::reg(format!("structowned{}", destination.0)),
            ));
        }
    }
    for argument in arguments {
        if analysis.owned_string_results.contains(argument) {
            text.emit(LlvmInst::call(
                Void,
                "free",
                vec![(Ptr, value_reg(*argument))],
            ));
        }
    }
}

/// Calls the override whose class name matches the receiver's runtime class,
/// or `fallback` when none does.
#[allow(clippy::too_many_arguments)]
fn emit_virtual_method_call(
    text: &mut String,
    state: &mut EmissionState,
    destination: ValueId,
    fallback: &str,
    overrides: &[(&str, &str)],
    args: &[Argument],
    ret: &LlvmType,
    receiver: ValueId,
) {
    let reg = LlvmOperand::reg;
    let n = state.continuation_count;
    state.continuation_count += 1;
    text.assign(format!("vcls{n}"), LlvmInst::load(Ptr, value_reg(receiver)));
    let join = format!("vjoin{n}");
    let fallback_label = format!("vfallback{n}");
    let call =
        |symbol: &str| LlvmInst::call(ret.clone(), &llvm_function_symbol(symbol), args.to_vec());
    let mut incoming = Vec::new();
    for (index, (class, target)) in overrides.iter().enumerate() {
        let label = format!("vcase{n}_{index}");
        let next = if index + 1 == overrides.len() {
            fallback_label.clone()
        } else {
            format!("vnext{n}_{index}")
        };
        let class_global = LlvmOperand::global(format!(".bn_cls_{}", sanitize_symbol(class)));
        text.assign(
            format!("veq{n}_{index}"),
            STR_EQ.call([reg(format!("vcls{n}")), class_global]),
        );
        text.assign(
            format!("vhit{n}_{index}"),
            LlvmInst::icmp(
                ICmpCond::Ne,
                I32,
                reg(format!("veq{n}_{index}")),
                LlvmOperand::int(0),
            ),
        );
        text.emit(LlvmInst::CondBr {
            cond: reg(format!("vhit{n}_{index}")),
            true_dest: label.clone(),
            false_dest: next.clone(),
        });
        state.control_flow.label(text, label.clone());
        if *ret == Void {
            text.emit(call(target));
        } else {
            text.assign(format!("vtmp{n}_{index}"), call(target));
            incoming.push((reg(format!("vtmp{n}_{index}")), label));
        }
        text.emit(LlvmInst::Br { dest: join.clone() });
        if index + 1 != overrides.len() {
            state.control_flow.label(text, next);
        }
    }
    state.control_flow.label(text, fallback_label.clone());
    if *ret == Void {
        text.emit(call(fallback));
        text.emit(LlvmInst::Br { dest: join.clone() });
        state.control_flow.label(text, join);
    } else {
        text.assign(format!("vfb{n}"), call(fallback));
        incoming.push((reg(format!("vfb{n}")), fallback_label));
        text.emit(LlvmInst::Br { dest: join.clone() });
        state.control_flow.label(text, join);
        text.assign(
            format!("v{}", destination.0),
            LlvmInst::Phi {
                ty: ret.clone(),
                incoming,
            },
        );
    }
}
