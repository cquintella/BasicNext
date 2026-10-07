// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The counted `FOR` continuation test: ascending while `current <= end` for a
// positive step, descending while `current >= end` otherwise.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    InstSink, LlvmInst, LlvmOperand,
    LlvmType::{I1, I64},
};

pub(crate) fn lower_for_condition(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let current = arguments[0];
    let end = arguments[1];
    let step = arguments[2];
    let current_type = analysis
        .values
        .get(&current)
        .expect("validated FOR current type");
    let step_type = analysis.values.get(&step).expect("validated FOR step type");
    let current_i64 = LlvmOperand::raw(extend_to_i64(text, current, current_type));
    let end_i64 = LlvmOperand::raw(extend_to_i64(
        text,
        end,
        analysis.values.get(&end).expect("validated FOR end type"),
    ));
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    text.assign(
        format!("for_step_positive{dest}"),
        LlvmInst::icmp(
            integer_compare_cond("Greater", step_type),
            crate::layout::typed_llvm(llvm_type(step_type).expect("validated FOR step type")),
            value_reg(step),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("for_ascending{dest}"),
        LlvmInst::icmp(
            integer_compare_cond("LessEqual", current_type),
            I64,
            current_i64.clone(),
            end_i64.clone(),
        ),
    );
    text.assign(
        format!("for_descending{dest}"),
        LlvmInst::icmp(
            integer_compare_cond("GreaterEqual", current_type),
            I64,
            current_i64,
            end_i64,
        ),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::select(
            reg(format!("for_step_positive{dest}")),
            I1,
            reg(format!("for_ascending{dest}")),
            reg(format!("for_descending{dest}")),
        ),
    );
}
