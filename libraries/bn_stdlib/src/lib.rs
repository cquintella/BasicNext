#![allow(clippy::not_unsafe_ptr_arg_deref)] // C ABI exports validate nullable out-pointers.
#![allow(clippy::single_match_else)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native C ABI and shared runtime modules for Basic Next `BN*` libraries
//! (`BNCrypto`, `BNData`, `BNDispatch`, `BNFfi`, `BNJson`, `BNLog`, `BNMath`,
//! `BNSqlite`), isolated from the language core (`crates/bn_rt`).

use std::ffi::c_char;

pub use bn_rt::*;

pub mod crypto;
mod crypto_abi;
pub mod crypto_error;
pub mod data_error;
mod dataframe;
mod dataframe_abi;
mod dispatch_abi;
pub mod dispatch_error;
pub mod dispatch_sync;
mod dispatch_sync_abi;
#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "ffi")]
pub(crate) mod ffi_abi;
#[cfg(feature = "ffi")]
pub(crate) mod ffi_loader;
#[cfg(all(test, feature = "ffi"))]
mod ffi_tests;
#[cfg(feature = "json")]
pub(crate) mod json_abi;
mod log_abi;
mod math;
mod math_abi;
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite_abi;
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite_error;
mod stats;

pub use bn_core_log::{Level as LogLevel, Record as LogRecord};
pub use crypto_abi::*;
pub use dataframe::{
    ConvertedCell, DataFrameColumn, DataFrameJoin, DataFrameJoinConfig, DataFrameResource,
    DataProvider, StandardDataProvider, add_dataframe_column, append_columns, append_rows,
    column_name, conversion_failed, convert_cell, convert_dataframe_column, copy_dataframe_column,
    copy_integer_element, dataframe_reduce_column, frame_from_csv_rows, get_dataframe_cell,
    join_dataframes, select_dataframe, set_column_label, slice_dataframe, transpose_dataframe,
    zscore_column,
};
pub use dataframe_abi::*;
pub use dispatch_abi::*;
pub use dispatch_sync_abi::*;
pub use log_abi::*;
pub use math_abi::*;
pub use stats::{Reduction, reduce};

#[allow(unsafe_code)] // C ABI: the column name is a borrowed NUL-terminated STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_add_integer_start(
    frame: u64,
    name: *const c_char,
    length: u32,
) -> i32 {
    let Some(name) = c_str(name) else {
        return -1;
    };
    dataframe_abi::add_integer_column_storage(frame, name, length)
        .and_then(|index| {
            i32::try_from(index).map_err(|_| dataframe_abi::BN_DATAFRAME_CONTRACT_ERROR)
        })
        .unwrap_or_else(|status| -i32::try_from(status).unwrap_or(1))
}

#[allow(unsafe_code)] // C ABI export; all arguments are scalar values.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_set_integer_cell(
    frame: u64,
    column: i32,
    row: u32,
    value: i64,
) -> u32 {
    let Ok(column) = u32::try_from(column) else {
        return dataframe_abi::BN_DATAFRAME_INVALID_ARGUMENT;
    };
    dataframe_abi::set_integer_cell_storage(frame, column, row, value)
}

#[allow(unsafe_code)] // C ABI export; returned storage is released by the LLVM caller.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_column_name_owned(frame: u64, index: u32) -> *mut c_char {
    dataframe_abi::column_name_storage(frame, index)
        .map_or(std::ptr::null_mut(), |name| c_string(&name))
}
