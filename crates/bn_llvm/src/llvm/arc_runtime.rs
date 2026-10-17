// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The native glue to the shared ARC core (proposal `arc-shared-core-0.6.5`):
//! small LLVM functions that read an object's core id from its header and
//! call `bn_rt`. Counting and liveness live only in `bn_rt::arc`; the rules
//! (when to retain and release) only in the validated IR; the destruction
//! sequence of each class only in its lowered `DESTRUCTOR` and `$release`.
//!
//! Header of an object or a region: `+0` the class-name constant
//! (`@.bn_cls_*`, null for a region), `+8` the core id.

use std::collections::BTreeSet;

use bn_ir::{FunctionKind, Module};

use super::{llvm_function_symbol, sanitize_symbol};
use crate::ir::{BasicBlock, ICmpCond, LlvmFunction, LlvmInst, LlvmOperand, LlvmType};
use crate::runtime_abi::{
    ARC_ADDRESS, ARC_DECLARATIONS, ARC_FINISH_DESTROY, ARC_RELEASE, ARC_RETAIN, TRAP_REPORT_FAILURE,
};

/// Byte offset of the core id in an object or region header.
pub(crate) const ID_OFFSET: i64 = crate::layout::POINTER_BYTES as i64;

/// A `STOP` outside `Start` (0.6.md, "`STOP`"): the function stores its
/// code, raises the flag, and returns; each caller's `$stopping` check reads
/// and clears the flag, releases its locals, and stops in turn.
pub(crate) const STOPPING_GLOBAL: &str = ".bn_stopping";
pub(crate) const STOP_CODE_GLOBAL: &str = ".bn_stop_code";

/// The definitions of the two `STOP` globals.
pub(crate) fn emit_stop_globals(text: &mut crate::ir::LlvmModule) {
    use crate::ir::{Linkage, LlvmGlobal};
    let private =
        |name: &str, ty, init| LlvmGlobal::variable(name, ty, init).with_linkage(Linkage::Private);
    text.global(private(
        STOPPING_GLOBAL,
        LlvmType::I1,
        LlvmOperand::bool(false),
    ));
    text.global(private(
        STOP_CODE_GLOBAL,
        LlvmType::I32,
        LlvmOperand::int(0),
    ));
}

/// In a dispatch trampoline, after the task returns: a task that stopped
/// ends the program with its code (a task's caller is the runtime, so no BN
/// function is left to release its locals).
pub(crate) fn stop_exit_check(text: &mut crate::ir::LlvmModule) {
    use crate::ir::InstSink;
    text.assign(
        "dispatch_stopping",
        LlvmInst::load(LlvmType::I1, LlvmOperand::global(STOPPING_GLOBAL)),
    );
    text.emit(LlvmInst::CondBr {
        cond: reg("dispatch_stopping"),
        true_dest: "dispatch_stopped".into(),
        false_dest: "dispatch_went_on".into(),
    });
    text.label("dispatch_stopped");
    text.assign(
        "dispatch_stop_code",
        LlvmInst::load(LlvmType::I32, LlvmOperand::global(STOP_CODE_GLOBAL)),
    );
    text.emit(LlvmInst::call(
        LlvmType::Void,
        "exit",
        vec![(LlvmType::I32, reg("dispatch_stop_code"))],
    ));
    text.emit(LlvmInst::Unreachable);
    text.label("dispatch_went_on");
}

/// Glue entry points the emitted code calls (each takes the header base).
pub(crate) const RETAIN: &str = "bn_arc_retain";
pub(crate) const RELEASE: &str = "bn_arc_release";
pub(crate) const WEAK_ID: &str = "bn_arc_weak_id";
pub(crate) const WEAK_READ: &str = "bn_arc_weak_read";
const DESTROY: &str = "bn_arc_destroy";

fn reg(name: &str) -> LlvmOperand {
    LlvmOperand::reg(name)
}

/// `%id = load i64` from the header of `%object`.
fn load_id(block: &mut BasicBlock) {
    block.push_assign(
        "idptr",
        LlvmInst::GetElementPtr {
            inbounds: false,
            elem_ty: LlvmType::I8,
            ptr: reg("object"),
            indices: vec![(LlvmType::I64, LlvmOperand::int(ID_OFFSET))],
        },
    );
    block.push_assign("id", LlvmInst::load(LlvmType::I64, reg("idptr")));
}

/// A function of `%object` that returns at once for null and otherwise runs
/// `body` with `%id` loaded.
fn on_object(name: &str, body: impl FnOnce(&mut LlvmFunction)) -> LlvmFunction {
    // `line` and `column`: the source position, for `BN_ARC_TRACE`.
    let mut function = LlvmFunction::new_definition(
        name,
        LlvmType::Void,
        vec![
            (LlvmType::Ptr, "object".into()),
            (LlvmType::I32, "line".into()),
            (LlvmType::I32, "column".into()),
        ],
    );
    function.set_linkage("private");
    let entry = function.add_block("entry");
    entry.push_assign(
        "null",
        LlvmInst::icmp(
            ICmpCond::Eq,
            LlvmType::Ptr,
            reg("object"),
            LlvmOperand::null(),
        ),
    );
    entry.terminate(LlvmInst::CondBr {
        cond: reg("null"),
        true_dest: "done".into(),
        false_dest: "live".into(),
    });
    load_id(function.add_block("live"));
    body(&mut function);
    function
        .add_block("done")
        .terminate(LlvmInst::Ret { val: None });
    function
}

/// Ends the program after a broken ARC invariant the core recorded (the
/// validated IR never causes one).
fn invariant_block(function: &mut LlvmFunction) {
    let failed = function.add_block("failed");
    failed.push(TRAP_REPORT_FAILURE.call([LlvmOperand::null()]));
    failed.push(LlvmInst::call(
        LlvmType::Void,
        "exit",
        vec![(LlvmType::I32, LlvmOperand::int(1))],
    ));
    failed.terminate(LlvmInst::Unreachable);
}

/// Branches to `failed` when the core call in `%status` returned `-1`.
fn check_status(block: &mut BasicBlock, next: &str) {
    block.push_assign(
        "broken",
        LlvmInst::icmp(
            ICmpCond::Slt,
            LlvmType::I32,
            reg("status"),
            LlvmOperand::int(0),
        ),
    );
    block.terminate(LlvmInst::CondBr {
        cond: reg("broken"),
        true_dest: "failed".into(),
        false_dest: next.into(),
    });
}

fn retain() -> LlvmFunction {
    let mut function = on_object(RETAIN, |function| {
        let live = function.current_block_mut().expect("live block");
        live.push_assign(
            "status",
            ARC_RETAIN.call([reg("id"), reg("line"), reg("column")]),
        );
        check_status(live, "done");
    });
    invariant_block(&mut function);
    function
}

fn release() -> LlvmFunction {
    let mut function = on_object(RELEASE, |function| {
        let live = function.current_block_mut().expect("live block");
        live.push_assign(
            "status",
            ARC_RELEASE.call([reg("id"), reg("line"), reg("column")]),
        );
        check_status(live, "counted");
        let counted = function.add_block("counted");
        counted.push_assign(
            "last",
            LlvmInst::icmp(
                ICmpCond::Eq,
                LlvmType::I32,
                reg("status"),
                LlvmOperand::int(1),
            ),
        );
        counted.terminate(LlvmInst::CondBr {
            cond: reg("last"),
            true_dest: "destroy".into(),
            false_dest: "done".into(),
        });
        let destroy = function.add_block("destroy");
        destroy.push(LlvmInst::call(
            LlvmType::Void,
            DESTROY,
            vec![(LlvmType::Ptr, reg("object"))],
        ));
        destroy.terminate(LlvmInst::Br {
            dest: "done".into(),
        });
    });
    invariant_block(&mut function);
    function
}

/// The destruction of an object whose last strong reference is gone,
/// selected by the class in its header (the dynamic class, so a derived
/// object held as its base runs the derived chain): its `DESTRUCTOR`, then
/// its `$release`; then the core frees the id and the memory is freed. A
/// region (null class) has neither.
fn destroy(module: &Module, allocated: &BTreeSet<String>) -> LlvmFunction {
    let mut function = LlvmFunction::new_definition(
        DESTROY,
        LlvmType::Void,
        vec![(LlvmType::Ptr, "object".into())],
    );
    function.set_linkage("private");
    let entry = function.add_block("entry");
    entry.push_assign("class", LlvmInst::load(LlvmType::Ptr, reg("object")));
    entry.terminate(LlvmInst::Br {
        dest: "case0".into(),
    });
    let classes = module
        .functions
        .iter()
        .filter(|function| function.kind == FunctionKind::ReleaseFields)
        .filter_map(|function| function.owner.as_deref())
        .filter(|owner| allocated.contains(*owner))
        .collect::<BTreeSet<_>>();
    for (index, class) in classes.iter().enumerate() {
        let test = function.add_block(format!("case{index}"));
        test.push_assign(
            format!("is{index}"),
            LlvmInst::icmp(
                ICmpCond::Eq,
                LlvmType::Ptr,
                reg("class"),
                LlvmOperand::global(format!(".bn_cls_{}", sanitize_symbol(class))),
            ),
        );
        test.terminate(LlvmInst::CondBr {
            cond: reg(&format!("is{index}")),
            true_dest: format!("run{index}"),
            false_dest: format!("case{}", index + 1),
        });
        let run = function.add_block(format!("run{index}"));
        for kind in [FunctionKind::Destructor, FunctionKind::ReleaseFields] {
            if let Some(callee) = module.function_of_kind(kind, class) {
                run.push(LlvmInst::call(
                    LlvmType::Void,
                    &llvm_function_symbol(&callee.name),
                    vec![(LlvmType::Ptr, reg("object"))],
                ));
            }
        }
        for (v_idx, offset) in super::layout::vector_field_offsets(module, class)
            .into_iter()
            .enumerate()
        {
            let fptr = format!("vfield_ptr_{index}_{v_idx}");
            let fval = format!("vfield_val_{index}_{v_idx}");
            let fbuf = format!("vfield_buf_{index}_{v_idx}");
            run.push_assign(
                &fptr,
                LlvmInst::GetElementPtr {
                    inbounds: false,
                    elem_ty: LlvmType::I8,
                    ptr: reg("object"),
                    indices: vec![(LlvmType::I32, LlvmOperand::int(i64::from(offset)))],
                },
            );
            run.push_assign(
                &fval,
                LlvmInst::load(crate::layout::vector_ty(), reg(&fptr)),
            );
            run.push_assign(
                &fbuf,
                LlvmInst::extract(
                    crate::layout::vector_ty(),
                    reg(&fval),
                    crate::layout::VECTOR_BUFFER_FIELD_INDEX,
                ),
            );
            run.push(LlvmInst::call(
                LlvmType::Void,
                "free",
                vec![(LlvmType::Ptr, reg(&fbuf))],
            ));
        }
        run.terminate(LlvmInst::Br {
            dest: "free".into(),
        });
    }
    function
        .add_block(format!("case{}", classes.len()))
        .terminate(LlvmInst::Br {
            dest: "free".into(),
        });
    let free = function.add_block("free");
    load_id(free);
    free.push_assign("status", ARC_FINISH_DESTROY.call([reg("id")]));
    check_status(free, "freed");
    let freed = function.add_block("freed");
    freed.push(LlvmInst::call(
        LlvmType::Void,
        "free",
        vec![(LlvmType::Ptr, reg("object"))],
    ));
    freed.terminate(LlvmInst::Ret { val: None });
    invariant_block(&mut function);
    function
}

/// What a weak binding stores for the object at `%object`: its core id, or
/// zero for null.
fn weak_id() -> LlvmFunction {
    let mut function = LlvmFunction::new_definition(
        WEAK_ID,
        LlvmType::I64,
        vec![(LlvmType::Ptr, "object".into())],
    );
    function.set_linkage("private");
    let entry = function.add_block("entry");
    entry.push_assign(
        "null",
        LlvmInst::icmp(
            ICmpCond::Eq,
            LlvmType::Ptr,
            reg("object"),
            LlvmOperand::null(),
        ),
    );
    entry.terminate(LlvmInst::CondBr {
        cond: reg("null"),
        true_dest: "none".into(),
        false_dest: "live".into(),
    });
    let live = function.add_block("live");
    load_id(live);
    live.terminate(LlvmInst::Ret {
        val: Some((LlvmType::I64, reg("id"))),
    });
    function.add_block("none").terminate(LlvmInst::Ret {
        val: Some((LlvmType::I64, LlvmOperand::int(0))),
    });
    function
}

/// What a weak binding holding `%id` reads: the object while it is alive,
/// else null.
fn weak_read() -> LlvmFunction {
    let mut function =
        LlvmFunction::new_definition(WEAK_READ, LlvmType::Ptr, vec![(LlvmType::I64, "id".into())]);
    function.set_linkage("private");
    let entry = function.add_block("entry");
    entry.push_assign("object", ARC_ADDRESS.call([reg("id")]));
    entry.terminate(LlvmInst::Ret {
        val: Some((LlvmType::Ptr, reg("object"))),
    });
    function
}

pub(crate) fn struct_copy_witness_name(owner: &str) -> String {
    format!("__bn_copy_{}", sanitize_symbol(owner))
}

pub(crate) fn struct_destroy_witness_name(owner: &str) -> String {
    format!("__bn_destroy_{}", sanitize_symbol(owner))
}

fn element_storage_bytes(module: &Module, ty: &bn_types::Type) -> u64 {
    if let Some(owner) = super::layout::struct_owner(module, ty) {
        return super::layout::class_instance_bytes(module, &owner);
    }
    match super::layout::llvm_type(ty).unwrap_or(LlvmType::I32) {
        LlvmType::I1 | LlvmType::I8 => u64::from(crate::layout::I8_BYTES),
        LlvmType::I16 => u64::from(crate::layout::I16_BYTES),
        LlvmType::I32 | LlvmType::Float => u64::from(crate::layout::I32_BYTES),
        LlvmType::I64 | LlvmType::Double | LlvmType::Ptr => u64::from(crate::layout::POINTER_BYTES),
        other
            if crate::layout::is_one_of(
                &other,
                &[
                    crate::layout::vector_ty(),
                    crate::layout::optional_float_ty(),
                    crate::layout::status_pointer_ty(),
                ],
            ) =>
        {
            u64::from(crate::layout::FAT_POINTER_BYTES)
        }
        _ => u64::from(crate::layout::POINTER_BYTES),
    }
}

fn copy_witness(module: &Module, owner: &str) -> LlvmFunction {
    let mut function = LlvmFunction::new_definition(
        struct_copy_witness_name(owner),
        LlvmType::Void,
        vec![(LlvmType::Ptr, "dst".into()), (LlvmType::Ptr, "src".into())],
    );
    function.set_linkage("private");
    let mut curr_blk = function.add_block("entry");
    let bytes = super::layout::class_instance_bytes(module, owner);
    curr_blk.push(LlvmInst::call(
        LlvmType::Void,
        crate::runtime_abi::memmove().name,
        vec![
            (LlvmType::Ptr, reg("dst")),
            (LlvmType::Ptr, reg("src")),
            (LlvmType::I64, LlvmOperand::uint(bytes)),
            (LlvmType::I1, LlvmOperand::bool(false)),
        ],
    ));

    for field in super::layout::struct_managed_fields(module, owner) {
        let offset = super::layout::field_byte_offset(module, &field.reference).unwrap_or(0);
        let slot_val = field.reference.slot.value();
        let dst_field = format!("dst_fld_{slot_val}");
        let src_field = format!("src_fld_{slot_val}");
        curr_blk.push_assign(
            &dst_field,
            LlvmInst::GetElementPtr {
                inbounds: false,
                elem_ty: LlvmType::I8,
                ptr: reg("dst"),
                indices: vec![(LlvmType::I32, LlvmOperand::int(i64::from(offset)))],
            },
        );
        curr_blk.push_assign(
            &src_field,
            LlvmInst::GetElementPtr {
                inbounds: false,
                elem_ty: LlvmType::I8,
                ptr: reg("src"),
                indices: vec![(LlvmType::I32, LlvmOperand::int(i64::from(offset)))],
            },
        );

        match &field.ty {
            bn_types::Type::Vector {
                element,
                dimensions,
            } => {
                let src_fat = format!("src_fat_{slot_val}");
                curr_blk.push_assign(
                    &src_fat,
                    LlvmInst::load(crate::layout::vector_ty(), reg(&src_field)),
                );
                let src_buf = format!("src_buf_{slot_val}");
                curr_blk.push_assign(
                    &src_buf,
                    LlvmInst::extract(
                        crate::layout::vector_ty(),
                        reg(&src_fat),
                        crate::layout::VECTOR_BUFFER_FIELD_INDEX,
                    ),
                );
                let src_len = format!("src_len_{slot_val}");
                curr_blk.push_assign(
                    &src_len,
                    LlvmInst::extract(
                        crate::layout::vector_ty(),
                        reg(&src_fat),
                        crate::layout::VECTOR_LENGTH_FIELD_INDEX,
                    ),
                );
                let is_null = format!("src_null_{slot_val}");
                curr_blk.push_assign(
                    &is_null,
                    LlvmInst::icmp(
                        ICmpCond::Eq,
                        LlvmType::Ptr,
                        reg(&src_buf),
                        LlvmOperand::null(),
                    ),
                );

                let copy_blk_name = format!("vec_copy_{slot_val}");
                let done_blk_name = format!("vec_done_{slot_val}");
                curr_blk.terminate(LlvmInst::CondBr {
                    cond: reg(&is_null),
                    true_dest: done_blk_name.clone(),
                    false_dest: copy_blk_name.clone(),
                });

                let copy_blk = function.add_block(copy_blk_name);
                let count =
                    usize::try_from(dimensions.iter().copied().product::<u64>()).unwrap_or(0);
                let elem_bytes = element_storage_bytes(module, element);
                let total_bytes = (count as u64).saturating_mul(elem_bytes);
                let dst_buf = format!("dst_buf_{slot_val}");
                copy_blk.push_assign(
                    &dst_buf,
                    LlvmInst::call(
                        LlvmType::Ptr,
                        "malloc",
                        vec![(LlvmType::I64, LlvmOperand::uint(total_bytes))],
                    ),
                );

                if let Some(elem_owner) = super::layout::struct_owner(module, element) {
                    for i in 0..count {
                        let offset_i = (i as u64).saturating_mul(elem_bytes);
                        let dst_elem = format!("dst_elem_{slot_val}_{i}");
                        let src_elem = format!("src_elem_{slot_val}_{i}");
                        copy_blk.push_assign(
                            &dst_elem,
                            LlvmInst::GetElementPtr {
                                inbounds: false,
                                elem_ty: LlvmType::I8,
                                ptr: reg(&dst_buf),
                                indices: vec![(
                                    LlvmType::I64,
                                    LlvmOperand::int(i64::try_from(offset_i).unwrap_or(0)),
                                )],
                            },
                        );
                        copy_blk.push_assign(
                            &src_elem,
                            LlvmInst::GetElementPtr {
                                inbounds: false,
                                elem_ty: LlvmType::I8,
                                ptr: reg(&src_buf),
                                indices: vec![(
                                    LlvmType::I64,
                                    LlvmOperand::int(i64::try_from(offset_i).unwrap_or(0)),
                                )],
                            },
                        );
                        copy_blk.push(LlvmInst::call(
                            LlvmType::Void,
                            &struct_copy_witness_name(&elem_owner),
                            vec![
                                (LlvmType::Ptr, reg(&dst_elem)),
                                (LlvmType::Ptr, reg(&src_elem)),
                            ],
                        ));
                    }
                } else if super::arc_ops::holds_references(module, element) {
                    copy_blk.push(LlvmInst::call(
                        LlvmType::Void,
                        crate::runtime_abi::memmove().name,
                        vec![
                            (LlvmType::Ptr, reg(&dst_buf)),
                            (LlvmType::Ptr, reg(&src_buf)),
                            (LlvmType::I64, LlvmOperand::uint(total_bytes)),
                            (LlvmType::I1, LlvmOperand::bool(false)),
                        ],
                    ));
                    for i in 0..count {
                        let offset_i = (i as u64).saturating_mul(elem_bytes);
                        let elem_ptr = format!("elem_ptr_{slot_val}_{i}");
                        copy_blk.push_assign(
                            &elem_ptr,
                            LlvmInst::GetElementPtr {
                                inbounds: false,
                                elem_ty: LlvmType::I8,
                                ptr: reg(&dst_buf),
                                indices: vec![(
                                    LlvmType::I64,
                                    LlvmOperand::int(i64::try_from(offset_i).unwrap_or(0)),
                                )],
                            },
                        );
                        let elem_obj = format!("elem_obj_{slot_val}_{i}");
                        copy_blk
                            .push_assign(&elem_obj, LlvmInst::load(LlvmType::Ptr, reg(&elem_ptr)));
                        copy_blk.push(LlvmInst::call(
                            LlvmType::Void,
                            RETAIN,
                            vec![
                                (LlvmType::Ptr, reg(&elem_obj)),
                                (LlvmType::I32, LlvmOperand::int(0)),
                                (LlvmType::I32, LlvmOperand::int(0)),
                            ],
                        ));
                    }
                } else {
                    copy_blk.push(LlvmInst::call(
                        LlvmType::Void,
                        crate::runtime_abi::memmove().name,
                        vec![
                            (LlvmType::Ptr, reg(&dst_buf)),
                            (LlvmType::Ptr, reg(&src_buf)),
                            (LlvmType::I64, LlvmOperand::uint(total_bytes)),
                            (LlvmType::I1, LlvmOperand::bool(false)),
                        ],
                    ));
                }

                let new_fat0 = format!("new_fat0_{slot_val}");
                copy_blk.push_assign(
                    &new_fat0,
                    LlvmInst::insert(
                        crate::layout::vector_ty(),
                        LlvmOperand::undef(),
                        LlvmType::Ptr,
                        reg(&dst_buf),
                        crate::layout::VECTOR_BUFFER_FIELD_INDEX,
                    ),
                );
                let new_fat = format!("new_fat_{slot_val}");
                copy_blk.push_assign(
                    &new_fat,
                    LlvmInst::insert(
                        crate::layout::vector_ty(),
                        reg(&new_fat0),
                        LlvmType::I32,
                        reg(&src_len),
                        crate::layout::VECTOR_LENGTH_FIELD_INDEX,
                    ),
                );
                copy_blk.push(LlvmInst::store(
                    crate::layout::vector_ty(),
                    reg(&new_fat),
                    reg(&dst_field),
                ));
                copy_blk.terminate(LlvmInst::Br {
                    dest: done_blk_name.clone(),
                });

                curr_blk = function.add_block(done_blk_name);
            }
            nested if super::layout::struct_owner(module, nested).is_some() => {
                let nested_owner = super::layout::struct_owner(module, nested).expect("owner");
                curr_blk.push(LlvmInst::call(
                    LlvmType::Void,
                    &struct_copy_witness_name(&nested_owner),
                    vec![
                        (LlvmType::Ptr, reg(&dst_field)),
                        (LlvmType::Ptr, reg(&src_field)),
                    ],
                ));
            }
            _ => {}
        }
    }

    curr_blk.terminate(LlvmInst::Ret { val: None });
    function
}

fn destroy_witness(module: &Module, owner: &str) -> LlvmFunction {
    let mut function = LlvmFunction::new_definition(
        struct_destroy_witness_name(owner),
        LlvmType::Void,
        vec![(LlvmType::Ptr, "ptr".into())],
    );
    function.set_linkage("private");
    let entry = function.add_block("entry");
    entry.push_assign(
        "is_null",
        LlvmInst::icmp(ICmpCond::Eq, LlvmType::Ptr, reg("ptr"), LlvmOperand::null()),
    );
    entry.terminate(LlvmInst::CondBr {
        cond: reg("is_null"),
        true_dest: "ret".into(),
        false_dest: "live".into(),
    });

    let mut curr_blk = function.add_block("live");
    let mut managed_fields = super::layout::struct_managed_fields(module, owner);
    managed_fields.reverse();

    for field in managed_fields {
        let offset = super::layout::field_byte_offset(module, &field.reference).unwrap_or(0);
        let slot_val = field.reference.slot.value();
        let field_ptr = format!("fld_ptr_{slot_val}");
        curr_blk.push_assign(
            &field_ptr,
            LlvmInst::GetElementPtr {
                inbounds: false,
                elem_ty: LlvmType::I8,
                ptr: reg("ptr"),
                indices: vec![(LlvmType::I32, LlvmOperand::int(i64::from(offset)))],
            },
        );

        match &field.ty {
            bn_types::Type::Vector {
                element,
                dimensions,
            } => {
                let fat = format!("vec_fat_{slot_val}");
                curr_blk.push_assign(
                    &fat,
                    LlvmInst::load(crate::layout::vector_ty(), reg(&field_ptr)),
                );
                let buf = format!("vec_buf_{slot_val}");
                curr_blk.push_assign(
                    &buf,
                    LlvmInst::extract(
                        crate::layout::vector_ty(),
                        reg(&fat),
                        crate::layout::VECTOR_BUFFER_FIELD_INDEX,
                    ),
                );
                let buf_null = format!("buf_null_{slot_val}");
                curr_blk.push_assign(
                    &buf_null,
                    LlvmInst::icmp(ICmpCond::Eq, LlvmType::Ptr, reg(&buf), LlvmOperand::null()),
                );

                let free_blk_name = format!("free_{slot_val}");
                let done_blk_name = format!("done_{slot_val}");
                curr_blk.terminate(LlvmInst::CondBr {
                    cond: reg(&buf_null),
                    true_dest: done_blk_name.clone(),
                    false_dest: free_blk_name.clone(),
                });

                let free_blk = function.add_block(free_blk_name);
                let count =
                    usize::try_from(dimensions.iter().copied().product::<u64>()).unwrap_or(0);
                let elem_bytes = element_storage_bytes(module, element);

                if let Some(elem_owner) = super::layout::struct_owner(module, element) {
                    for i in (0..count).rev() {
                        let offset_i = (i as u64).saturating_mul(elem_bytes);
                        let elem_ptr = format!("elem_ptr_{slot_val}_{i}");
                        free_blk.push_assign(
                            &elem_ptr,
                            LlvmInst::GetElementPtr {
                                inbounds: false,
                                elem_ty: LlvmType::I8,
                                ptr: reg(&buf),
                                indices: vec![(
                                    LlvmType::I64,
                                    LlvmOperand::int(i64::try_from(offset_i).unwrap_or(0)),
                                )],
                            },
                        );
                        free_blk.push(LlvmInst::call(
                            LlvmType::Void,
                            &struct_destroy_witness_name(&elem_owner),
                            vec![(LlvmType::Ptr, reg(&elem_ptr))],
                        ));
                    }
                } else if super::arc_ops::holds_references(module, element) {
                    for i in (0..count).rev() {
                        let offset_i = (i as u64).saturating_mul(elem_bytes);
                        let elem_ptr = format!("elem_ptr_{slot_val}_{i}");
                        free_blk.push_assign(
                            &elem_ptr,
                            LlvmInst::GetElementPtr {
                                inbounds: false,
                                elem_ty: LlvmType::I8,
                                ptr: reg(&buf),
                                indices: vec![(
                                    LlvmType::I64,
                                    LlvmOperand::int(i64::try_from(offset_i).unwrap_or(0)),
                                )],
                            },
                        );
                        let elem_obj = format!("elem_obj_{slot_val}_{i}");
                        free_blk
                            .push_assign(&elem_obj, LlvmInst::load(LlvmType::Ptr, reg(&elem_ptr)));
                        free_blk.push(LlvmInst::call(
                            LlvmType::Void,
                            RELEASE,
                            vec![
                                (LlvmType::Ptr, reg(&elem_obj)),
                                (LlvmType::I32, LlvmOperand::int(0)),
                                (LlvmType::I32, LlvmOperand::int(0)),
                            ],
                        ));
                    }
                }

                free_blk.push(LlvmInst::call(
                    LlvmType::Void,
                    "free",
                    vec![(LlvmType::Ptr, reg(&buf))],
                ));
                free_blk.push(LlvmInst::store(
                    crate::layout::vector_ty(),
                    LlvmOperand::zero_initializer(),
                    reg(&field_ptr),
                ));
                free_blk.terminate(LlvmInst::Br {
                    dest: done_blk_name.clone(),
                });

                curr_blk = function.add_block(done_blk_name);
            }
            nested if super::layout::struct_owner(module, nested).is_some() => {
                let nested_owner = super::layout::struct_owner(module, nested).expect("owner");
                curr_blk.push(LlvmInst::call(
                    LlvmType::Void,
                    &struct_destroy_witness_name(&nested_owner),
                    vec![(LlvmType::Ptr, reg(&field_ptr))],
                ));
            }
            _ => {}
        }
    }

    curr_blk.terminate(LlvmInst::Br { dest: "ret".into() });
    function
        .add_block("ret")
        .terminate(LlvmInst::Ret { val: None });
    function
}

/// The ARC glue of a module whose objects and regions use the core;
/// `allocated` names the classes `NEW` creates (their class constants are
/// defined by the preamble).
pub(crate) fn emit_glue(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    allocated: &BTreeSet<String>,
) {
    crate::ir::write_declarations(text, ARC_DECLARATIONS);
    for function in [
        retain(),
        release(),
        destroy(module, allocated),
        weak_id(),
        weak_read(),
    ] {
        text.blank();
        text.function(function);
    }
    emit_witness_glue(text, module);
}

pub(crate) fn emit_witness_glue(text: &mut crate::ir::LlvmModule, module: &Module) {
    if !super::layout::all_struct_owners(module).is_empty() {
        crate::ir::write_declarations(
            text,
            &[&crate::runtime_abi::FREE, &crate::runtime_abi::MALLOC],
        );
    }
    for owner in super::layout::all_struct_owners(module) {
        text.blank();
        text.function(copy_witness(module, &owner));
        text.blank();
        text.function(destroy_witness(module, &owner));
    }
}
