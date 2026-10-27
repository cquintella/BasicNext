// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI of `DataFrame.CountNA`, `DropNA` and `FillNA` (bndata.md, "Missing
//! values"); the rule is `dataframe::na`, which `bni` calls too.

use std::ffi::c_char;

use super::data_error::DataFailure;
use super::dataframe::{
    CellKind, FillCells, count_not_available, drop_not_available, fill_not_available,
};
use super::dataframe_abi::{
    BN_DATAFRAME_OK, BNDataFrameHandle, BNDataFrameStatus, StoredValue, failed, input_string,
    next_handle, with_frames,
};

/// The operation names the runtime records with a failure.
const COUNT_NA: &str = "BNData.DataFrame.CountNA";
const DROP_NA: &str = "BNData.DataFrame.DropNA";
const FILL_NA: &str = "BNData.DataFrame.FillNA";

/// How the native cells read and are made for `FillNA`.
const CELLS: FillCells<StoredValue> = FillCells {
    kind: cell_kind,
    integer: |number| StoredValue::Integer(i64::from(number)),
    float: StoredValue::Float,
};

fn cell_kind(cell: &StoredValue) -> CellKind {
    match cell {
        StoredValue::NotAvailable => CellKind::NotAvailable,
        StoredValue::Integer(_) => CellKind::Integer,
        StoredValue::Float(_) => CellKind::Float,
        _ => CellKind::Other,
    }
}

fn is_not_available(cell: &StoredValue) -> bool {
    matches!(cell, StoredValue::NotAvailable)
}

/// `CountNA(name)`: writes the count to `out_count`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_count_na(
    frame: BNDataFrameHandle,
    name: *const c_char,
    out_count: *mut i64,
) -> BNDataFrameStatus {
    let Some(name) = input_string(name) else {
        return failed(COUNT_NA, &DataFailure::InvalidHandle);
    };
    if out_count.is_null() {
        return failed(COUNT_NA, &DataFailure::InvalidHandle);
    }
    let counted = with_frames(|frames| {
        frames
            .get(&frame)
            .ok_or(DataFailure::InvalidHandle)
            .and_then(|resource| count_not_available(resource, &name, is_not_available))
    });
    match counted {
        Ok(count) => {
            let count = i64::try_from(count).expect("a column holds fewer than 2^63 cells");
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { out_count.write(count) };
            BN_DATAFRAME_OK
        }
        Err(failure) => failed(COUNT_NA, &failure),
    }
}

/// `DropNA()`: writes the new frame's handle to `out_frame`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_drop_na(
    frame: BNDataFrameHandle,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    if out_frame.is_null() {
        return failed(DROP_NA, &DataFailure::InvalidHandle);
    }
    let kept = with_frames(|frames| {
        frames
            .get(&frame)
            .map(|resource| drop_not_available(resource, is_not_available))
    });
    let Some(kept) = kept else {
        return failed(DROP_NA, &DataFailure::InvalidHandle);
    };
    let handle = next_handle();
    with_frames(|frames| {
        frames.insert(handle, kept);
    });
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { out_frame.write(handle) };
    BN_DATAFRAME_OK
}

/// `FillNA(name, value)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_fill_na(
    frame: BNDataFrameHandle,
    name: *const c_char,
    value: f64,
) -> BNDataFrameStatus {
    let Some(name) = input_string(name) else {
        return failed(FILL_NA, &DataFailure::InvalidHandle);
    };
    let filled = with_frames(|frames| {
        frames
            .get_mut(&frame)
            .ok_or(DataFailure::InvalidHandle)
            .and_then(|resource| fill_not_available(resource, &name, value, &CELLS))
    });
    filled.map_or_else(|failure| failed(FILL_NA, &failure), |()| BN_DATAFRAME_OK)
}
