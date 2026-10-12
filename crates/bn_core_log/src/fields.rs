// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The fields of a record: the map a record carries and the bound of one
//! value (bnlog.md, "Records").

use std::collections::BTreeMap;

use crate::catalog;
use crate::error::LogFailure;

/// Bytes of one field value at most (UTF-8).
pub const FIELD_VALUE_MAX_BYTES: usize = 4096;

/// Field key to value, in key order.
pub type FieldMap = BTreeMap<String, String>;

/// `Ok` when every value of `fields` is within `FIELD_VALUE_MAX_BYTES`.
///
/// # Errors
///
/// `InvalidString` naming the first value past the bound.
pub fn check_values(fields: &FieldMap) -> Result<(), LogFailure> {
    match fields
        .values()
        .find(|value| value.len() > FIELD_VALUE_MAX_BYTES)
    {
        Some(value) => Err(LogFailure::InvalidString {
            what: catalog::text("what.field_value"),
            len: value.len(),
            max: FIELD_VALUE_MAX_BYTES,
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bn_types::error_codes::log;

    #[test]
    fn a_value_past_the_bound_is_refused() {
        let at_bound = FieldMap::from([("k".into(), "v".repeat(FIELD_VALUE_MAX_BYTES))]);
        assert!(check_values(&at_bound).is_ok());
        let past = FieldMap::from([("k".into(), "v".repeat(FIELD_VALUE_MAX_BYTES + 1))]);
        assert_eq!(
            check_values(&past).unwrap_err().code(),
            log::INVALID_ARGUMENT
        );
    }
}
