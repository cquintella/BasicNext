// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The codes the native math ABI passes for a numeric vector: the element
//! type and the descriptive statistic (`value-memory-abi.md`, `BNMath`
//! vector reductions). `bnc` writes them and the runtime reads them, so both sides
//! use this one table. A code is a position here and is never reused.

use crate::{FloatType, IntegerType, Type};

/// The element type of a numeric vector or region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericElement {
    Integer(IntegerType),
    Float(FloatType),
}

const ELEMENTS: [NumericElement; 10] = [
    NumericElement::Integer(IntegerType::Byte),
    NumericElement::Integer(IntegerType::Int8),
    NumericElement::Integer(IntegerType::Int16),
    NumericElement::Integer(IntegerType::Int32),
    NumericElement::Integer(IntegerType::Int64),
    NumericElement::Integer(IntegerType::UInt16),
    NumericElement::Integer(IntegerType::UInt32),
    NumericElement::Integer(IntegerType::UInt64),
    NumericElement::Float(FloatType::Float32),
    NumericElement::Float(FloatType::Float64),
];

/// The `BNMath` statistics, by the member name; a code is a position.
const STATISTICS: [&str; 14] = [
    "MEAN",
    "MEDIAN",
    "QUARTILE1",
    "QUARTILE3",
    "RANGE",
    "STDEV",
    "VARIANCE",
    "MODE",
    "PVariance",
    "PStdev",
    "Skewness",
    "Kurtosis",
    "PSkewness",
    "PKurtosis",
];

fn position_code(position: usize) -> i32 {
    i32::try_from(position).expect("the code tables are short")
}

impl NumericElement {
    /// The element type `ty` denotes, when it is an integer or floating type.
    #[must_use]
    pub const fn of(ty: &Type) -> Option<Self> {
        match ty {
            Type::Integer(kind) => Some(Self::Integer(*kind)),
            Type::Float(kind) => Some(Self::Float(*kind)),
            _ => None,
        }
    }

    /// The ABI code of this element type.
    ///
    /// # Panics
    ///
    /// Never: every element type is in the code table.
    #[must_use]
    pub fn code(self) -> i32 {
        position_code(
            ELEMENTS
                .iter()
                .position(|element| *element == self)
                .expect("every element type is in the table"),
        )
    }

    /// The element type an ABI code denotes.
    #[must_use]
    pub fn from_code(code: i32) -> Option<Self> {
        usize::try_from(code)
            .ok()
            .and_then(|position| ELEMENTS.get(position).copied())
    }
}

/// The ABI code of the `BNMath` statistic named `name` (`"MEAN"`, …).
#[must_use]
pub fn statistic_code(name: &str) -> Option<i32> {
    STATISTICS
        .iter()
        .position(|statistic| *statistic == name)
        .map(position_code)
}

/// The `BNMath` statistic name an ABI code denotes.
#[must_use]
pub fn statistic_name(code: i32) -> Option<&'static str> {
    usize::try_from(code)
        .ok()
        .and_then(|position| STATISTICS.get(position).copied())
}
