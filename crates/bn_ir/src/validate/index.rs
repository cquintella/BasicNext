// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Which result type an `Index` may produce for a given object type.

use bn_types::Type;

use super::upcast::types_compatible;

/// The element of `POINTER TO T OR NULL`, when `result` is that element.
fn nullable_pointer_element(members: &[Type], result: &Type) -> bool {
    if members.len() != 2 || !members.iter().any(|member| matches!(member, Type::Null)) {
        return false;
    }
    let Some(Type::Pointer { element, .. }) = members
        .iter()
        .find(|member| matches!(member, Type::Pointer { .. }))
    else {
        return false;
    };
    types_compatible(element, result)
}

pub(super) fn index_result_matches(object: &Type, result: &Type) -> bool {
    match object {
        Type::Vector {
            element,
            dimensions,
        } => {
            let expected = if dimensions.len() > 1 {
                Type::Vector {
                    element: element.clone(),
                    dimensions: dimensions[1..].to_vec(),
                }
            } else {
                element.as_ref().clone()
            };
            types_compatible(&expected, result)
        }
        Type::Pointer { element, .. } => types_compatible(element, result),
        // `POINTER TO T OR NULL` indexes as its pointer element. A value typed
        // exactly `NULL` falls through and is not indexable; the null
        // alternative is a runtime trap, not this type.
        Type::Alternative(members) => nullable_pointer_element(members, result),
        Type::String | Type::HostArgs => matches!(result, Type::String),
        // Lowering may preserve an unresolved synthesized value. Its concrete
        // index contract is checked once the producer's type is known.
        Type::Unknown => true,
        _ => false,
    }
}
