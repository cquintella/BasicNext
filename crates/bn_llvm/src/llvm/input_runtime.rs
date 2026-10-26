// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bn_input`: one line from stdin as a STRING, or the EOF payload.

use crate::ir::{ICmpCond, LlvmFunction, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::string_payload;

fn reg(name: &str) -> O {
    O::reg(name)
}

/// `bn_input(reusable)`: `reusable` is ignored. The runtime allocates the
/// line; the IR releases the previous STRING. End of input selects the
/// `@.bn_eof` payload.
pub(crate) fn input_function() -> LlvmFunction {
    let mut function =
        LlvmFunction::new_definition("bn_input", T::Ptr, vec![(T::Ptr, "input.reusable".into())]);
    let entry = function.add_block("entry");
    entry.push_assign(
        "input.line",
        I::call(
            T::Ptr,
            "bn_rt_input_line",
            vec![(T::Ptr, reg("input.reusable"))],
        ),
    );
    entry.push_assign(
        "input.isnull",
        I::icmp(ICmpCond::Eq, T::Ptr, reg("input.line"), O::null()),
    );
    entry.push_assign(
        "input.result",
        I::select(
            reg("input.isnull"),
            T::Ptr,
            string_payload(".bn_eof"),
            reg("input.line"),
        ),
    );
    entry.push(I::Ret {
        val: Some((T::Ptr, reg("input.result"))),
    });
    function
}

/// `bn_input_str(reusable)`: same as `bn_input`, but returns the `@.bn_empty`
/// payload on end-of-file.
pub(crate) fn input_str_function() -> LlvmFunction {
    let mut function = LlvmFunction::new_definition(
        "bn_input_str",
        T::Ptr,
        vec![(T::Ptr, "input.reusable".into())],
    );
    let entry = function.add_block("entry");
    entry.push_assign(
        "input.line",
        I::call(
            T::Ptr,
            "bn_rt_input_line",
            vec![(T::Ptr, reg("input.reusable"))],
        ),
    );
    entry.push_assign(
        "input.isnull",
        I::icmp(ICmpCond::Eq, T::Ptr, reg("input.line"), O::null()),
    );
    entry.push_assign(
        "input.result",
        I::select(
            reg("input.isnull"),
            T::Ptr,
            string_payload(".bn_empty"),
            reg("input.line"),
        ),
    );
    entry.push(I::Ret {
        val: Some((T::Ptr, reg("input.result"))),
    });
    function
}
