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
use std::fmt::Write as _;

use bn_ir::{FunctionKind, Module};

use super::{llvm_function_symbol, sanitize_symbol};
use crate::ir::{
    AbiType::{I32, I64, Ptr, Void},
    BasicBlock, ICmpCond, LlvmFunction, LlvmInst, LlvmOperand, LlvmType, RuntimeFn,
};

/// Byte offset of the core id in an object or region header.
pub(crate) const ID_OFFSET: i64 = 8;

pub(crate) const ARC_REGISTER: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_arc_register",
    ret: I64,
    params: [Ptr, Ptr, I32, I32],
};
const ARC_RETAIN: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_arc_retain",
    ret: I32,
    params: [I64, I32, I32],
};
const ARC_RELEASE: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_arc_release",
    ret: I32,
    params: [I64, I32, I32],
};
const ARC_FINISH_DESTROY: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_arc_finish_destroy",
    ret: I32,
    params: [I64],
};
const ARC_ADDRESS: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_arc_address",
    ret: Ptr,
    params: [I64],
};
const TRAP_REPORT: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_trap_report_failure",
    ret: Void,
    params: [Ptr],
};

/// A `STOP` outside `Start` (0.6.md, "`STOP`"): the function stores its
/// code, raises the flag, and returns; each caller's `$stopping` check reads
/// and clears the flag, releases its locals, and stops in turn.
pub(crate) const STOPPING_GLOBAL: &str = ".bn_stopping";
pub(crate) const STOP_CODE_GLOBAL: &str = ".bn_stop_code";

/// The definitions of the two `STOP` globals.
pub(crate) fn stop_globals() -> String {
    format!(
        "@{STOPPING_GLOBAL} = private global i1 false\n@{STOP_CODE_GLOBAL} = private global i32 0\n"
    )
}

/// In a dispatch trampoline, after the task returns: a task that stopped
/// ends the program with its code (a task's caller is the runtime, so no BN
/// function is left to release its locals).
pub(crate) fn stop_exit_check(text: &mut String) {
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
    failed.push(TRAP_REPORT.call([LlvmOperand::null()]));
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

/// The ARC glue of a module whose objects and regions use the core;
/// `allocated` names the classes `NEW` creates (their class constants are
/// defined by the preamble).
pub(crate) fn glue(module: &Module, allocated: &BTreeSet<String>) -> String {
    use crate::ir::Declaration;
    let mut text = String::new();
    for declaration in [
        &ARC_REGISTER as &dyn Declaration,
        &ARC_RETAIN,
        &ARC_RELEASE,
        &ARC_FINISH_DESTROY,
        &ARC_ADDRESS,
    ] {
        let _ = writeln!(text, "{}", declaration.declaration());
    }
    for function in [
        retain(),
        release(),
        destroy(module, allocated),
        weak_id(),
        weak_read(),
    ] {
        let _ = write!(text, "\n{function}");
    }
    text
}
