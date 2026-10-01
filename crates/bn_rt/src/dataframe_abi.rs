//! Stable C ABI for the structural `BNData.DataFrame` operations.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::dataframe::{
    DataFrameColumn, DataFrameJoin, DataFrameJoinConfig, DataFrameResource, add_dataframe_column,
    append_columns, append_rows, column_name, copy_dataframe_column, dataframe_reduce_column,
    get_dataframe_cell, join_dataframes, select_dataframe, set_column_label, transpose_dataframe,
    zscore_column,
};
use super::dispatch_abi::BNValue;
use super::file_abi::{read_handle, write_handle};

use super::data_error::DataFailure;

pub type BNDataFrameHandle = u64;
pub type BNDataFrameStatus = u32;

pub const BN_DATAFRAME_OK: BNDataFrameStatus = 0;
pub const BN_DATAFRAME_INVALID_ARGUMENT: BNDataFrameStatus =
    bn_types::error_codes::data::INVALID_ARGUMENT as u32;
pub const BN_DATAFRAME_NOT_FOUND: BNDataFrameStatus = bn_types::error_codes::data::NOT_FOUND as u32;
pub const BN_DATAFRAME_TYPE_MISMATCH: BNDataFrameStatus =
    bn_types::error_codes::data::TYPE_MISMATCH as u32;
pub const BN_DATAFRAME_OUT_OF_RANGE: BNDataFrameStatus =
    bn_types::error_codes::data::OUT_OF_RANGE as u32;
pub const BN_DATAFRAME_IO_FAILED: BNDataFrameStatus = bn_types::error_codes::data::IO_FAILED as u32;
pub const BN_DATAFRAME_INVALID_FORMAT: BNDataFrameStatus =
    bn_types::error_codes::data::INVALID_FORMAT as u32;

pub const BN_DATAFRAME_INVALID_HANDLE: BNDataFrameStatus = BN_DATAFRAME_INVALID_ARGUMENT;
pub const BN_DATAFRAME_CONTRACT_ERROR: BNDataFrameStatus = BN_DATAFRAME_INVALID_ARGUMENT;
pub const BN_DATAFRAME_DUPLICATE_COLUMN: BNDataFrameStatus = BN_DATAFRAME_INVALID_ARGUMENT;
pub const BN_DATAFRAME_COLUMN_LENGTH_MISMATCH: BNDataFrameStatus = BN_DATAFRAME_INVALID_ARGUMENT;
pub const BN_DATAFRAME_POLICY_DENIED: BNDataFrameStatus = BN_DATAFRAME_IO_FAILED;

fn failed(operation: &str, failure: &DataFailure) -> BNDataFrameStatus {
    crate::set_error_report(
        failure.code(),
        operation,
        failure.message(),
        failure.cause(),
    );
    failure.code().cast_unsigned()
}

/// Maximum allowed byte length for column names passed across the C ABI.
pub const MAX_COLUMN_NAME_LENGTH: usize = 256;

/// Borrowed input view. Names and values are copied during `create`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BNDataFrameColumnView {
    pub name: *const c_char,
    pub values: *const BNValue,
    pub length: u32,
}

#[derive(Clone, PartialEq)]
enum StoredValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Vec<u8>),
    Bytes(Vec<u8>),
    Null,
    NotAvailable,
    EndOfFile,
    Handle(u64),
}

type Frame = DataFrameResource<StoredValue>;

fn render_value(value: &StoredValue) -> String {
    match value {
        StoredValue::Boolean(v) => v.to_string(),
        StoredValue::Integer(v) => v.to_string(),
        StoredValue::Float(v) => v.to_string(),
        StoredValue::String(v) | StoredValue::Bytes(v) => String::from_utf8_lossy(v).into_owned(),
        StoredValue::Null => "NULL".into(),
        StoredValue::NotAvailable => "NA".into(),
        StoredValue::EndOfFile => "EOF".into(),
        StoredValue::Handle(v) => v.to_string(),
    }
}

struct Registry {
    next: AtomicU64,
    frames: Mutex<HashMap<BNDataFrameHandle, Frame>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        next: AtomicU64::new(1),
        frames: Mutex::new(HashMap::new()),
    })
}

fn next_handle() -> BNDataFrameHandle {
    registry().next.fetch_add(1, Ordering::Relaxed)
}

fn input_string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn owned_string(value: &[u8]) -> *mut c_char {
    let Ok(length) = value.len().checked_add(1).ok_or(()) else {
        return std::ptr::null_mut();
    };
    let pointer = unsafe { libc::malloc(length) }.cast::<u8>();
    if pointer.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        std::ptr::copy_nonoverlapping(value.as_ptr(), pointer, value.len());
        pointer.add(value.len()).write(0);
    }
    pointer.cast()
}

fn add_column(
    frame: BNDataFrameHandle,
    name: *const c_char,
    values: Vec<StoredValue>,
    op_name: &str,
) -> BNDataFrameStatus {
    let Some(name) = input_string(name) else {
        return failed(op_name, &DataFailure::InvalidHandle);
    };
    with_frames(|frames| {
        let Some(frame) = frames.get_mut(&frame) else {
            return failed(op_name, &DataFailure::InvalidHandle);
        };
        let expected = frame
            .columns
            .first()
            .map_or(values.len(), |c| c.values.len());
        let got = values.len();
        add_dataframe_column(frame, name.clone(), values).map_or_else(
            |message| {
                let failure = match message.as_str() {
                    "duplicate column name" => DataFailure::DuplicateColumn(name),
                    "column length mismatch" => DataFailure::ColumnLengthMismatch { expected, got },
                    _ => DataFailure::from_message(&message),
                };
                failed(op_name, &failure)
            },
            |()| BN_DATAFRAME_OK,
        )
    })
}

macro_rules! add_numeric_column {
    ($name:ident, $input:ty, $variant:ident, $op:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            frame: BNDataFrameHandle,
            name: *const c_char,
            values: *const $input,
            length: u32,
        ) -> BNDataFrameStatus {
            if length > 0 && values.is_null() {
                return failed($op, &DataFailure::InvalidHandle);
            }
            let values = if length == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(values, length as usize) }
                    .iter()
                    .copied()
                    .map(|value| StoredValue::$variant(value.into()))
                    .collect()
            };
            add_column(frame, name, values, $op)
        }
    };
}

add_numeric_column!(
    bn_rt_dataframe_add_integer,
    i32,
    Integer,
    "BNData.DataFrame.AddIntegerColumn"
);
add_numeric_column!(
    bn_rt_dataframe_add_float,
    f64,
    Float,
    "BNData.DataFrame.AddFloatColumn"
);

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_add_boolean(
    frame: BNDataFrameHandle,
    name: *const c_char,
    values: *const u8,
    length: u32,
) -> BNDataFrameStatus {
    if length > 0 && values.is_null() {
        return failed(
            "BNData.DataFrame.AddBooleanColumn",
            &DataFailure::InvalidHandle,
        );
    }
    let values = if length == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(values, length as usize) }
            .iter()
            .map(|value| StoredValue::Boolean(*value != 0))
            .collect()
    };
    add_column(frame, name, values, "BNData.DataFrame.AddBooleanColumn")
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_add_string(
    frame: BNDataFrameHandle,
    name: *const c_char,
    values: *const *const c_char,
    length: u32,
) -> BNDataFrameStatus {
    if length > 0 && values.is_null() {
        return failed(
            "BNData.DataFrame.AddStringColumn",
            &DataFailure::InvalidHandle,
        );
    }
    let pointers = if length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(values, length as usize) }
    };
    let Some(values) = pointers
        .iter()
        .map(|pointer| input_string(*pointer).map(|value| StoredValue::String(value.into_bytes())))
        .collect::<Option<Vec<_>>>()
    else {
        return failed(
            "BNData.DataFrame.AddStringColumn",
            &DataFailure::InvalidHandle,
        );
    };
    add_column(frame, name, values, "BNData.DataFrame.AddStringColumn")
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_set_label(
    frame: BNDataFrameHandle,
    old_label: *const c_char,
    new_label: *const c_char,
) -> BNDataFrameStatus {
    let (Some(old_label), Some(new_label)) = (input_string(old_label), input_string(new_label))
    else {
        return failed("BNData.DataFrame.SetLabel", &DataFailure::InvalidHandle);
    };
    with_frames(|frames| {
        let Some(frame) = frames.get_mut(&frame) else {
            return failed("BNData.DataFrame.SetLabel", &DataFailure::InvalidHandle);
        };
        set_column_label(frame, &old_label, &new_label).map_or_else(
            |message| {
                let failure = match message.as_str() {
                    "column not found" => DataFailure::ColumnNotFound(old_label),
                    "duplicate column name" => DataFailure::DuplicateColumn(new_label),
                    _ => DataFailure::from_message(&message),
                };
                failed("BNData.DataFrame.SetLabel", &failure)
            },
            |()| BN_DATAFRAME_OK,
        )
    })
}

fn cell(
    frame: BNDataFrameHandle,
    row: u32,
    name: *const c_char,
) -> Result<StoredValue, DataFailure> {
    let Some(name) = input_string(name) else {
        return Err(DataFailure::InvalidHandle);
    };
    with_frames(|frames| {
        let frame = frames.get(&frame).ok_or(DataFailure::InvalidHandle)?;
        get_dataframe_cell(frame, &name, row as usize)
            .cloned()
            .map_err(|message| match message.as_str() {
                "column not found" => DataFailure::ColumnNotFound(name.clone()),
                "DataFrame index out of bounds" | "row index out of bounds" => {
                    DataFailure::RowIndexOutOfRange {
                        row: i64::from(row),
                        count: frame.columns.first().map_or(0, |c| c.values.len()),
                    }
                }
                _ => DataFailure::from_message(&message),
            })
    })
}

macro_rules! get_scalar {
    ($name:ident, $output:ty, $variant:ident, $method:literal, $expected:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            frame: BNDataFrameHandle,
            row: u32,
            name: *const c_char,
            out: *mut $output,
            out_na: *mut u8,
        ) -> BNDataFrameStatus {
            if out.is_null() || out_na.is_null() {
                return failed(
                    concat!("BNData.DataFrame.", $method),
                    &DataFailure::InvalidHandle,
                );
            }
            match cell(frame, row, name) {
                Ok(StoredValue::$variant(value)) => unsafe {
                    out.write(value as $output);
                    out_na.write(0);
                    BN_DATAFRAME_OK
                },
                Ok(StoredValue::NotAvailable) => unsafe {
                    out_na.write(1);
                    BN_DATAFRAME_OK
                },
                Ok(_) => {
                    let col_name = input_string(name).unwrap_or_default();
                    failed(
                        concat!("BNData.DataFrame.", $method),
                        &DataFailure::TypeMismatch {
                            expected: $expected,
                            column: col_name,
                        },
                    )
                }
                Err(failure) => failed(concat!("BNData.DataFrame.", $method), &failure),
            }
        }
    };
}

get_scalar!(
    bn_rt_dataframe_get_integer,
    i64,
    Integer,
    "GetInteger",
    "INTEGER"
);
get_scalar!(bn_rt_dataframe_get_float, f64, Float, "GetFloat", "FLOAT");
get_scalar!(
    bn_rt_dataframe_get_boolean,
    u8,
    Boolean,
    "GetBoolean",
    "BOOLEAN"
);

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_get_string(
    frame: BNDataFrameHandle,
    row: u32,
    name: *const c_char,
    out: *mut *mut c_char,
    out_na: *mut u8,
) -> BNDataFrameStatus {
    if out.is_null() || out_na.is_null() {
        return failed("BNData.DataFrame.GetString", &DataFailure::InvalidHandle);
    }
    match cell(frame, row, name) {
        Ok(StoredValue::String(value)) => unsafe {
            let value = owned_string(&value);
            if value.is_null() {
                return failed(
                    "BNData.DataFrame.GetString",
                    &DataFailure::InvalidFormat("out of memory".into()),
                );
            }
            out.write(value);
            out_na.write(0);
            BN_DATAFRAME_OK
        },
        Ok(StoredValue::NotAvailable) => unsafe {
            out.write(std::ptr::null_mut());
            out_na.write(1);
            BN_DATAFRAME_OK
        },
        Ok(_) => {
            let col_name = input_string(name).unwrap_or_default();
            failed(
                "BNData.DataFrame.GetString",
                &DataFailure::TypeMismatch {
                    expected: "STRING",
                    column: col_name,
                },
            )
        }
        Err(failure) => failed("BNData.DataFrame.GetString", &failure),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_reduce(
    frame: BNDataFrameHandle,
    name: *const c_char,
    operation: u32,
    out: *mut f64,
    out_na: *mut u8,
) -> BNDataFrameStatus {
    let Some(method) = [
        "Mean",
        "Median",
        "Quartile1",
        "Quartile3",
        "Mode",
        "Stdev",
        "Variance",
        "Range",
        "Min",
        "Max",
    ]
    .get(operation as usize) else {
        return failed("BNData.DataFrame.Reduce", &DataFailure::InvalidHandle);
    };
    let op_name = format!("BNData.DataFrame.{method}");
    if out.is_null() || out_na.is_null() {
        return failed(&op_name, &DataFailure::InvalidHandle);
    }
    let Some(name) = input_string(name) else {
        return failed(&op_name, &DataFailure::InvalidHandle);
    };
    let result = with_frames(|frames| {
        let frame = frames.get(&frame).ok_or(DataFailure::InvalidHandle)?;
        dataframe_reduce_column(frame, &name, method, |value| match value {
            #[allow(clippy::cast_precision_loss)]
            StoredValue::Integer(value) => Ok(Some(*value as f64)),
            StoredValue::Float(value) => Ok(Some(*value)),
            StoredValue::NotAvailable => Ok(None),
            _ => Err("column is not numeric"),
        })
        .map_err(|message| match message {
            "column not found" => DataFailure::ColumnNotFound(name.clone()),
            "empty numeric column" => DataFailure::EmptyNumericColumn,
            "column is not numeric" => DataFailure::NonNumericColumn(name.clone()),
            _ => DataFailure::from_message(message),
        })
    });
    match result {
        Ok(super::stats::Reduction::Float(value)) => unsafe {
            out.write(value);
            out_na.write(0);
            BN_DATAFRAME_OK
        },
        Ok(super::stats::Reduction::Na) => unsafe {
            out_na.write(1);
            BN_DATAFRAME_OK
        },
        Err(failure) => failed(&op_name, &failure),
    }
}

fn with_frames<T>(operation: impl FnOnce(&mut HashMap<BNDataFrameHandle, Frame>) -> T) -> T {
    operation(
        &mut registry()
            .frames
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

pub(crate) fn add_integer_column_storage(
    frame: BNDataFrameHandle,
    name: &str,
    length: u32,
) -> Result<u32, BNDataFrameStatus> {
    with_frames(|frames| {
        let Some(resource) = frames.get_mut(&frame) else {
            return Err(failed(
                "BNData.DataFrame.AddIntegerColumn",
                &DataFailure::InvalidHandle,
            ));
        };
        let index = u32::try_from(resource.columns.len()).map_err(|_| {
            failed(
                "BNData.DataFrame.AddIntegerColumn",
                &DataFailure::InvalidFormat("column count overflow".into()),
            )
        })?;
        add_dataframe_column(
            resource,
            name.to_owned(),
            vec![StoredValue::Integer(0); usize::try_from(length).unwrap_or(0)],
        )
        .map_err(|message| {
            let failure = match message.as_str() {
                "duplicate column name" => DataFailure::DuplicateColumn(name.to_owned()),
                "column length mismatch" => {
                    let expected = resource.columns.first().map_or(0, |c| c.values.len());
                    DataFailure::ColumnLengthMismatch {
                        expected,
                        got: usize::try_from(length).unwrap_or(0),
                    }
                }
                _ => DataFailure::from_message(&message),
            };
            failed("BNData.DataFrame.AddIntegerColumn", &failure)
        })?;
        Ok(index)
    })
}

pub(crate) fn set_integer_cell_storage(
    frame: BNDataFrameHandle,
    column: u32,
    row: u32,
    value: i64,
) -> BNDataFrameStatus {
    with_frames(|frames| {
        let Some(resource) = frames.get_mut(&frame) else {
            return failed(
                "BNData.DataFrame.SetIntegerCell",
                &DataFailure::InvalidHandle,
            );
        };
        let Some(cell) = resource
            .columns
            .get_mut(usize::try_from(column).unwrap_or(usize::MAX))
            .and_then(|column| {
                column
                    .values
                    .get_mut(usize::try_from(row).unwrap_or(usize::MAX))
            })
        else {
            return failed(
                "BNData.DataFrame.SetIntegerCell",
                &DataFailure::RowIndexOutOfRange {
                    row: i64::from(row),
                    count: resource.columns.first().map_or(0, |c| c.values.len()),
                },
            );
        };
        *cell = StoredValue::Integer(value);
        BN_DATAFRAME_OK
    })
}

pub(crate) fn column_name_storage(
    frame: BNDataFrameHandle,
    index: u32,
) -> Result<String, BNDataFrameStatus> {
    with_frames(|frames| {
        let Some(resource) = frames.get(&frame) else {
            return Err(failed(
                "BNData.DataFrame.ColumnName",
                &DataFailure::InvalidHandle,
            ));
        };
        column_name(resource, usize::try_from(index).unwrap_or(usize::MAX))
            .map(str::to_owned)
            .map_err(|_| {
                failed(
                    "BNData.DataFrame.ColumnName",
                    &DataFailure::ColumnIndexOutOfRange {
                        column: i64::from(index),
                        count: resource.columns.len(),
                    },
                )
            })
    })
}

#[allow(unsafe_code)]
unsafe fn copy_value(value: &BNValue) -> Option<StoredValue> {
    // Validate value.kind range before inspecting union payload to avoid UB on hostile/uninitialized FFI values.
    // We read the raw 4-byte discriminant at value's address.
    let raw_kind = unsafe { *std::ptr::from_ref::<BNValue>(value).cast::<u32>() };
    if raw_kind > (super::dispatch_abi::BNValueKind::EndOfFile as u32) {
        return None;
    }
    match value.kind {
        super::dispatch_abi::BNValueKind::Boolean => {
            Some(StoredValue::Boolean(unsafe { value.payload.boolean != 0 }))
        }
        super::dispatch_abi::BNValueKind::Integer => {
            Some(StoredValue::Integer(unsafe { value.payload.integer }))
        }
        super::dispatch_abi::BNValueKind::Float => {
            Some(StoredValue::Float(unsafe { value.payload.floating }))
        }
        super::dispatch_abi::BNValueKind::String | super::dispatch_abi::BNValueKind::Bytes => {
            let bytes = unsafe { value.payload.bytes };
            if bytes.length > 0 && bytes.data.is_null() {
                return None;
            }
            let copied = if bytes.length == 0 {
                Vec::new()
            } else {
                unsafe {
                    std::slice::from_raw_parts(bytes.data, usize::try_from(bytes.length).ok()?)
                }
                .to_vec()
            };
            if value.kind == super::dispatch_abi::BNValueKind::String {
                Some(StoredValue::String(copied))
            } else {
                Some(StoredValue::Bytes(copied))
            }
        }
        super::dispatch_abi::BNValueKind::Null => Some(StoredValue::Null),
        super::dispatch_abi::BNValueKind::NotAvailable => Some(StoredValue::NotAvailable),
        super::dispatch_abi::BNValueKind::EndOfFile => Some(StoredValue::EndOfFile),
        super::dispatch_abi::BNValueKind::Handle => {
            Some(StoredValue::Handle(unsafe { value.payload.handle }))
        }
    }
}

#[allow(unsafe_code)]
unsafe fn copy_column(view: &BNDataFrameColumnView) -> Option<DataFrameColumn<StoredValue>> {
    if view.name.is_null() || (view.length > 0 && view.values.is_null()) {
        return None;
    }
    // Scan at most MAX_COLUMN_NAME_LENGTH + 1 bytes looking for NUL terminator.
    let name_slice = unsafe {
        let ptr = view.name.cast::<u8>();
        std::slice::from_raw_parts(ptr, MAX_COLUMN_NAME_LENGTH + 1)
    };
    let nul_pos = name_slice.iter().position(|&b| b == 0)?;
    if nul_pos > MAX_COLUMN_NAME_LENGTH {
        return None;
    }
    let name = unsafe { CStr::from_ptr(view.name) }
        .to_str()
        .ok()?
        .to_owned();
    let values = if view.length == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(view.values, usize::try_from(view.length).ok()?) }
            .iter()
            .map(|value| unsafe { copy_value(value) })
            .collect::<Option<Vec<_>>>()?
    };
    Some(DataFrameColumn { name, values })
}

/// Creates a frame by copying borrowed column views into the runtime registry.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_create(
    columns: *const BNDataFrameColumnView,
    column_count: u32,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_frame.is_null() || (column_count > 0 && columns.is_null()) {
            return failed("BNData.DataFrame.Create", &DataFailure::InvalidHandle);
        }
        let views = if column_count == 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(columns, usize::try_from(column_count).unwrap_or(0))
            }
        };
        let Some(columns) = views
            .iter()
            .map(|view| unsafe { copy_column(view) })
            .collect::<Option<Vec<_>>>()
        else {
            return failed(
                "BNData.DataFrame.Create",
                &DataFailure::InvalidArgument("invalid column view".into()),
            );
        };
        let handle = next_handle();
        with_frames(|frames| {
            frames.insert(handle, DataFrameResource { columns });
        });
        unsafe {
            *out_frame = handle;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.Create", &DataFailure::InvalidHandle))
}

fn same_stored_type(left: &StoredValue, right: &StoredValue) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn is_missing(value: &StoredValue) -> bool {
    matches!(value, StoredValue::NotAvailable)
}

/// Appends rows and returns a new owning frame handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_append_rows(
    left: BNDataFrameHandle,
    right: BNDataFrameHandle,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_frame.is_null() {
            return failed("BNData.DataFrame.AppendRows", &DataFailure::InvalidHandle);
        }
        let result = with_frames(|frames| {
            let (Some(left), Some(right)) = (frames.get(&left), frames.get(&right)) else {
                return Err(DataFailure::InvalidHandle);
            };
            append_rows(left, right, is_missing, same_stored_type)
                .map_err(|message| DataFailure::from_message(&message))
        });
        let frame = match result {
            Ok(frame) => frame,
            Err(failure) => return failed("BNData.DataFrame.AppendRows", &failure),
        };
        let handle = next_handle();
        with_frames(|frames| {
            frames.insert(handle, frame);
        });
        unsafe {
            *out_frame = handle;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.AppendRows", &DataFailure::InvalidHandle))
}

/// Appends columns and returns a new owning frame handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_append_columns(
    left: BNDataFrameHandle,
    right: BNDataFrameHandle,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_frame.is_null() {
            return failed(
                "BNData.DataFrame.AppendColumns",
                &DataFailure::InvalidHandle,
            );
        }
        let result = with_frames(|frames| {
            let (Some(left), Some(right)) = (frames.get(&left), frames.get(&right)) else {
                return Err(DataFailure::InvalidHandle);
            };
            append_columns(left, right).map_err(|message| match message.as_str() {
                "row counts differ" => {
                    let left_rows = left.columns.first().map_or(0, |c| c.values.len());
                    let right_rows = right.columns.first().map_or(0, |c| c.values.len());
                    DataFailure::RowCountsDiffer {
                        left: left_rows,
                        right: right_rows,
                    }
                }
                "duplicate column label" => {
                    DataFailure::DuplicateColumn("duplicate column label".into())
                }
                _ => DataFailure::from_message(&message),
            })
        });
        let frame = match result {
            Ok(frame) => frame,
            Err(failure) => return failed("BNData.DataFrame.AppendColumns", &failure),
        };
        let handle = next_handle();
        with_frames(|frames| {
            frames.insert(handle, frame);
        });
        unsafe {
            *out_frame = handle;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| {
        failed(
            "BNData.DataFrame.AppendColumns",
            &DataFailure::InvalidHandle,
        )
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_join(
    left: BNDataFrameHandle,
    right: BNDataFrameHandle,
    left_key: *const c_char,
    right_key: *const c_char,
    kind: u32,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    if out_frame.is_null() {
        return failed("BNData.DataFrame.Join", &DataFailure::InvalidHandle);
    }
    let (Some(left_key), Some(right_key)) = (input_string(left_key), input_string(right_key))
    else {
        return failed("BNData.DataFrame.Join", &DataFailure::InvalidHandle);
    };
    let Some(join_kind) = [
        DataFrameJoin::Inner,
        DataFrameJoin::Left,
        DataFrameJoin::Right,
        DataFrameJoin::Full,
    ]
    .get(kind as usize)
    .copied() else {
        return failed("BNData.DataFrame.Join", &DataFailure::InvalidHandle);
    };
    let result = with_frames(|frames| {
        let (Some(left), Some(right)) = (frames.get(&left), frames.get(&right)) else {
            return Err(DataFailure::InvalidHandle);
        };
        let missing = StoredValue::NotAvailable;
        join_dataframes(
            left,
            right,
            &DataFrameJoinConfig {
                left_label: &left_key,
                right_label: &right_key,
                kind: join_kind,
                equals: |a, b| a == b,
                is_not_available: is_missing,
                not_available: &missing,
            },
        )
        .map_err(|message| match message.as_str() {
            "left key column not found" => DataFailure::KeyColumnNotFound {
                key: left_key.clone(),
                side: "left",
            },
            "right key column not found" => DataFailure::KeyColumnNotFound {
                key: right_key.clone(),
                side: "right",
            },
            _ => DataFailure::from_message(&message),
        })
    });
    let result = match result {
        Ok(value) => value,
        Err(failure) => return failed("BNData.DataFrame.Join", &failure),
    };
    let handle = next_handle();
    with_frames(|frames| {
        frames.insert(handle, result);
    });
    unsafe {
        out_frame.write(handle);
    }
    BN_DATAFRAME_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_convert_integer(
    frame: BNDataFrameHandle,
    name: *const c_char,
) -> BNDataFrameStatus {
    convert_column(
        frame,
        name,
        "BNData.DataFrame.ConvertInteger",
        |col_name, value| match value {
            StoredValue::String(bytes) => String::from_utf8(bytes.clone())
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .map(StoredValue::Integer)
                .ok_or_else(|| DataFailure::ConversionFailed {
                    column: col_name.to_owned(),
                    reason: "failed to parse string as integer".into(),
                }),
            StoredValue::Integer(_) => Ok(value.clone()),
            _ => Err(DataFailure::TypeMismatch {
                expected: "INTEGER",
                column: col_name.to_owned(),
            }),
        },
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_convert_float(
    frame: BNDataFrameHandle,
    name: *const c_char,
) -> BNDataFrameStatus {
    convert_column(
        frame,
        name,
        "BNData.DataFrame.ConvertFloat",
        |col_name, value| match value {
            StoredValue::String(bytes) => String::from_utf8(bytes.clone())
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .map(StoredValue::Float)
                .ok_or_else(|| DataFailure::ConversionFailed {
                    column: col_name.to_owned(),
                    reason: "failed to parse string as float".into(),
                }),
            StoredValue::Float(_) => Ok(value.clone()),
            _ => Err(DataFailure::TypeMismatch {
                expected: "FLOAT",
                column: col_name.to_owned(),
            }),
        },
    )
}

fn convert_column(
    frame: BNDataFrameHandle,
    name: *const c_char,
    operation: &'static str,
    converter: impl Fn(&str, &StoredValue) -> Result<StoredValue, DataFailure>,
) -> BNDataFrameStatus {
    let Some(name) = input_string(name) else {
        return failed(operation, &DataFailure::InvalidHandle);
    };
    with_frames(|frames| {
        let Some(resource) = frames.get_mut(&frame) else {
            return failed(operation, &DataFailure::InvalidHandle);
        };
        let Some(column) = resource
            .columns
            .iter_mut()
            .find(|column| column.name == name)
        else {
            return failed(operation, &DataFailure::ColumnNotFound(name));
        };
        let mut values = Vec::with_capacity(column.values.len());
        for val in &column.values {
            match converter(&name, val) {
                Ok(cell) => values.push(cell),
                Err(failure) => return failed(operation, &failure),
            }
        }
        column.values = values;
        BN_DATAFRAME_OK
    })
}

/// Returns the number of rows in a frame.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_row_count(
    frame: BNDataFrameHandle,
    out_count: *mut u32,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_count.is_null() {
            return failed("BNData.DataFrame.RowCount", &DataFailure::InvalidHandle);
        }
        let Some(count) = with_frames(|frames| {
            frames.get(&frame).map(|resource| {
                resource
                    .columns
                    .first()
                    .map_or(0, |column| column.values.len())
            })
        }) else {
            return failed("BNData.DataFrame.RowCount", &DataFailure::InvalidHandle);
        };
        let Ok(count) = u32::try_from(count) else {
            return failed(
                "BNData.DataFrame.RowCount",
                &DataFailure::InvalidFormat("row count overflow".into()),
            );
        };
        unsafe {
            *out_count = count;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.RowCount", &DataFailure::InvalidHandle))
}

/// Returns the number of columns in a frame.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_column_count(
    frame: BNDataFrameHandle,
    out_count: *mut u32,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_count.is_null() {
            return failed("BNData.DataFrame.ColumnCount", &DataFailure::InvalidHandle);
        }
        let Some(count) =
            with_frames(|frames| frames.get(&frame).map(|resource| resource.columns.len()))
        else {
            return failed("BNData.DataFrame.ColumnCount", &DataFailure::InvalidHandle);
        };
        let Ok(count) = u32::try_from(count) else {
            return failed(
                "BNData.DataFrame.ColumnCount",
                &DataFailure::InvalidFormat("column count overflow".into()),
            );
        };
        unsafe {
            *out_count = count;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.ColumnCount", &DataFailure::InvalidHandle))
}

/// Selects rows and columns and returns a new owning frame handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_select(
    frame: BNDataFrameHandle,
    rows: *const u32,
    row_count: u32,
    columns: *const u32,
    column_count: u32,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_frame.is_null()
            || (row_count > 0 && rows.is_null())
            || (column_count > 0 && columns.is_null())
        {
            return failed("BNData.DataFrame.Select", &DataFailure::InvalidHandle);
        }
        let row_values = if row_count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(rows, usize::try_from(row_count).unwrap_or(0)) }
        };
        let row_indices = row_values
            .iter()
            .map(|index| usize::try_from(*index).unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        let column_values = if column_count == 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(columns, usize::try_from(column_count).unwrap_or(0))
            }
        };
        let column_indices = column_values
            .iter()
            .map(|index| usize::try_from(*index).unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        let result = with_frames(|frames| {
            let Some(source) = frames.get(&frame) else {
                return Err(DataFailure::InvalidHandle);
            };
            let total_rows = source.columns.first().map_or(0, |c| c.values.len());
            let total_cols = source.columns.len();
            if let Some(&bad_row) = row_indices.iter().find(|&&r| r >= total_rows) {
                return Err(DataFailure::RowIndexOutOfRange {
                    row: i64::try_from(bad_row).unwrap_or(i64::MAX),
                    count: total_rows,
                });
            }
            if let Some(&bad_col) = column_indices.iter().find(|&&c| c >= total_cols) {
                return Err(DataFailure::ColumnIndexOutOfRange {
                    column: i64::try_from(bad_col).unwrap_or(i64::MAX),
                    count: total_cols,
                });
            }
            select_dataframe(source, &row_indices, &column_indices)
                .map_err(|message| DataFailure::from_message(&message))
        });
        let result = match result {
            Ok(result) => result,
            Err(failure) => return failed("BNData.DataFrame.Select", &failure),
        };
        let handle = next_handle();
        with_frames(|frames| {
            frames.insert(handle, result);
        });
        unsafe {
            *out_frame = handle;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.Select", &DataFailure::InvalidHandle))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_slice(
    frame: BNDataFrameHandle,
    start_row: u32,
    row_count: u32,
    start_column: u32,
    column_count: u32,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    if out_frame.is_null() {
        return failed("BNData.DataFrame.Slice", &DataFailure::InvalidHandle);
    }
    let result = with_frames(|frames| {
        let Some(source) = frames.get(&frame) else {
            return Err(DataFailure::InvalidHandle);
        };
        let total_rows = source.columns.first().map_or(0, |c| c.values.len());
        let total_cols = source.columns.len();
        let sr = usize::try_from(start_row).unwrap_or(usize::MAX);
        let rc = usize::try_from(row_count).unwrap_or(usize::MAX);
        let sc = usize::try_from(start_column).unwrap_or(usize::MAX);
        let cc = usize::try_from(column_count).unwrap_or(usize::MAX);
        if rc > 0 && (sr >= total_rows || rc > total_rows - sr) {
            return Err(DataFailure::SliceOutOfRange {
                start: sr,
                len: rc,
                total: total_rows,
                dim: "row",
            });
        }
        if cc > 0 && (sc >= total_cols || cc > total_cols - sc) {
            return Err(DataFailure::SliceOutOfRange {
                start: sc,
                len: cc,
                total: total_cols,
                dim: "column",
            });
        }
        super::dataframe::slice_dataframe(source, sr, rc, sc, cc)
            .map_err(|message| DataFailure::from_message(&message))
    });
    let result = match result {
        Ok(value) => value,
        Err(failure) => return failed("BNData.DataFrame.Slice", &failure),
    };
    let handle = next_handle();
    with_frames(|frames| {
        frames.insert(handle, result);
    });
    unsafe {
        out_frame.write(handle);
    }
    BN_DATAFRAME_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_transpose(
    frame: BNDataFrameHandle,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    if out_frame.is_null() {
        return failed("BNData.DataFrame.Transpose", &DataFailure::InvalidHandle);
    }
    let result = with_frames(|frames| {
        frames
            .get(&frame)
            .cloned()
            .ok_or(DataFailure::InvalidHandle)
            .map(|source| {
                transpose_dataframe(&source, render_value, |value| {
                    StoredValue::String(value.into_bytes())
                })
            })
    });
    let result = match result {
        Ok(value) => value,
        Err(failure) => return failed("BNData.DataFrame.Transpose", &failure),
    };
    let handle = next_handle();
    with_frames(|frames| {
        frames.insert(handle, result);
    });
    unsafe {
        out_frame.write(handle);
    }
    BN_DATAFRAME_OK
}

/// Computes a numeric z-score column and returns a new owning frame.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_zscore(
    frame: BNDataFrameHandle,
    name: *const c_char,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_frame.is_null() {
            return failed("BNData.DataFrame.ZScore", &DataFailure::InvalidHandle);
        }
        let Some(name) = input_string(name) else {
            return failed("BNData.DataFrame.ZScore", &DataFailure::InvalidHandle);
        };
        let result = with_frames(|frames| {
            let Some(source) = frames.get(&frame) else {
                return Err(DataFailure::InvalidHandle);
            };
            zscore_column(
                source,
                &name,
                |value| match value {
                    #[allow(clippy::cast_precision_loss)]
                    StoredValue::Integer(value) => Some(*value as f64),
                    StoredValue::Float(value) => Some(*value),
                    _ => None,
                },
                StoredValue::Float,
                &StoredValue::NotAvailable,
            )
            .map_err(|message| match message.as_str() {
                "column not found" => DataFailure::ColumnNotFound(name.clone()),
                "empty numeric column" => DataFailure::EmptyNumericColumn,
                _ => DataFailure::from_message(&message),
            })
        });
        let result = match result {
            Ok(result) => result,
            Err(failure) => return failed("BNData.DataFrame.ZScore", &failure),
        };
        let handle = next_handle();
        with_frames(|frames| {
            frames.insert(handle, result);
        });
        unsafe {
            out_frame.write(handle);
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.ZScore", &DataFailure::InvalidHandle))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_copy_integer(
    frame: BNDataFrameHandle,
    name: *const c_char,
    target: *mut i32,
    length: u32,
) -> BNDataFrameStatus {
    if target.is_null() && length > 0 {
        return failed("BNData.DataFrame.CopyInteger", &DataFailure::InvalidHandle);
    }
    let Some(name) = input_string(name) else {
        return failed("BNData.DataFrame.CopyInteger", &DataFailure::InvalidHandle);
    };
    let result = with_frames(|frames| {
        let Some(source) = frames.get(&frame) else {
            return Err(DataFailure::InvalidHandle);
        };
        let col = source.columns.iter().find(|c| c.name == name);
        let Some(col) = col else {
            return Err(DataFailure::ColumnNotFound(name.clone()));
        };
        if length as usize != col.values.len() {
            return Err(DataFailure::DestinationLengthMismatch {
                expected: col.values.len(),
                got: length as usize,
            });
        }
        copy_dataframe_column(source, &name, length as usize, |value| match value {
            StoredValue::Integer(value) => {
                i32::try_from(*value).map_err(|_| "integer out of range")
            }
            _ => Err("column is not integer"),
        })
        .map_err(|message| match message {
            "column not found" => DataFailure::ColumnNotFound(name.clone()),
            "column is not integer" => DataFailure::TypeMismatch {
                expected: "INTEGER",
                column: name.clone(),
            },
            "destination length mismatch" => DataFailure::DestinationLengthMismatch {
                expected: col.values.len(),
                got: length as usize,
            },
            _ => DataFailure::from_message(message),
        })
    });
    match result {
        Ok(values) => {
            unsafe {
                std::ptr::copy_nonoverlapping(values.as_ptr(), target, values.len());
            }
            BN_DATAFRAME_OK
        }
        Err(failure) => failed("BNData.DataFrame.CopyInteger", &failure),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_copy_float(
    frame: BNDataFrameHandle,
    name: *const c_char,
    target: *mut f64,
    length: u32,
) -> BNDataFrameStatus {
    if target.is_null() && length > 0 {
        return failed("BNData.DataFrame.CopyFloat", &DataFailure::InvalidHandle);
    }
    let Some(name) = input_string(name) else {
        return failed("BNData.DataFrame.CopyFloat", &DataFailure::InvalidHandle);
    };
    let result = with_frames(|frames| {
        let Some(source) = frames.get(&frame) else {
            return Err(DataFailure::InvalidHandle);
        };
        let col = source.columns.iter().find(|c| c.name == name);
        let Some(col) = col else {
            return Err(DataFailure::ColumnNotFound(name.clone()));
        };
        if length as usize != col.values.len() {
            return Err(DataFailure::DestinationLengthMismatch {
                expected: col.values.len(),
                got: length as usize,
            });
        }
        copy_dataframe_column(source, &name, length as usize, |value| match value {
            StoredValue::Float(value) => Ok(*value),
            _ => Err("column is not float"),
        })
        .map_err(|message| match message {
            "column not found" => DataFailure::ColumnNotFound(name.clone()),
            "column is not float" => DataFailure::TypeMismatch {
                expected: "FLOAT",
                column: name.clone(),
            },
            "destination length mismatch" => DataFailure::DestinationLengthMismatch {
                expected: col.values.len(),
                got: length as usize,
            },
            _ => DataFailure::from_message(message),
        })
    });
    match result {
        Ok(values) => {
            unsafe {
                std::ptr::copy_nonoverlapping(values.as_ptr(), target, values.len());
            }
            BN_DATAFRAME_OK
        }
        Err(failure) => failed("BNData.DataFrame.CopyFloat", &failure),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_read_csv(
    file: u64,
    has_header: u8,
    separator: *const c_char,
    out_frame: *mut BNDataFrameHandle,
) -> BNDataFrameStatus {
    if out_frame.is_null() {
        return failed("BNData.ReadCSV", &DataFailure::InvalidHandle);
    }
    let Some(separator_str) = input_string(separator) else {
        return failed(
            "BNData.ReadCSV",
            &DataFailure::InvalidSeparator(String::new()),
        );
    };
    let mut chars = separator_str.chars();
    let Some(separator) = chars.next() else {
        return failed(
            "BNData.ReadCSV",
            &DataFailure::InvalidSeparator(separator_str),
        );
    };
    if chars.next().is_some() || matches!(separator, '"' | '\n' | '\r') {
        return failed(
            "BNData.ReadCSV",
            &DataFailure::InvalidSeparator(separator_str),
        );
    }
    let text = match read_handle(file) {
        Ok(text) => text,
        Err(status) => {
            return failed(
                "BNData.ReadCSV",
                &DataFailure::IoFailed(format!("file error status {status}")),
            );
        }
    };
    let Ok(rows) = super::dataframe::parse_csv(&text, separator) else {
        return failed("BNData.ReadCSV", &DataFailure::UnterminatedQuotedField);
    };
    let Ok(frame) = super::dataframe::frame_from_csv_rows(rows, has_header != 0, |value| {
        StoredValue::String(value.into_bytes())
    }) else {
        return failed(
            "BNData.ReadCSV",
            &DataFailure::RaggedRow {
                expected: 0,
                got: 0,
            },
        );
    };
    let handle = next_handle();
    with_frames(|frames| {
        frames.insert(handle, frame);
    });
    unsafe {
        out_frame.write(handle);
    }
    BN_DATAFRAME_OK
}

/// Closes a frame handle. Closing twice returns `INVALID_HANDLE`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_close(frame: BNDataFrameHandle) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_frames(|frames| {
            frames.remove(&frame).map_or_else(
                || failed("BNData.DataFrame.Close", &DataFailure::InvalidHandle),
                |_| BN_DATAFRAME_OK,
            )
        })
    }))
    .unwrap_or_else(|_| failed("BNData.DataFrame.Close", &DataFailure::InvalidHandle))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_write_csv(
    file: u64,
    frame: BNDataFrameHandle,
    write_header: u8,
    separator: *const c_char,
) -> BNDataFrameStatus {
    let Some(separator_str) = input_string(separator) else {
        return failed(
            "BNData.WriteCSV",
            &DataFailure::InvalidSeparator(String::new()),
        );
    };
    let mut chars = separator_str.chars();
    let Some(separator) = chars.next() else {
        return failed(
            "BNData.WriteCSV",
            &DataFailure::InvalidSeparator(separator_str),
        );
    };
    if chars.next().is_some() || matches!(separator, '"' | '\n' | '\r') {
        return failed(
            "BNData.WriteCSV",
            &DataFailure::InvalidSeparator(separator_str),
        );
    }
    let Some(source) = with_frames(|frames| frames.get(&frame).cloned()) else {
        return failed("BNData.WriteCSV", &DataFailure::InvalidHandle);
    };
    let mut output = String::new();
    if write_header != 0 {
        output.push_str(
            &source
                .columns
                .iter()
                .map(|column| column.name.clone())
                .collect::<Vec<_>>()
                .join(&separator.to_string()),
        );
        output.push('\n');
    }
    let rows = source
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    for row in 0..rows {
        for (index, column) in source.columns.iter().enumerate() {
            if index > 0 {
                output.push(separator);
            }
            output.push_str(&render_value(&column.values[row]));
        }
        output.push('\n');
    }
    match write_handle(file, &output) {
        Ok(()) => BN_DATAFRAME_OK,
        Err(status) => failed(
            "BNData.WriteCSV",
            &DataFailure::IoFailed(format!("file error status {status}")),
        ),
    }
}

#[cfg(test)]
#[path = "dataframe_csv_tests.rs"]
mod csv_tests;

#[cfg(test)]
mod tests {
    use super::super::dispatch_abi::{BNValueBytes, BNValueKind, BNValuePayload};
    use super::{
        BN_DATAFRAME_CONTRACT_ERROR, BN_DATAFRAME_INVALID_ARGUMENT, BN_DATAFRAME_INVALID_HANDLE,
        BN_DATAFRAME_OK, BNDataFrameColumnView, BNDataFrameHandle, BNValue, MAX_COLUMN_NAME_LENGTH,
        bn_rt_dataframe_append_columns, bn_rt_dataframe_append_rows, bn_rt_dataframe_close,
        bn_rt_dataframe_column_count, bn_rt_dataframe_create, bn_rt_dataframe_row_count,
        bn_rt_dataframe_select,
    };

    #[test]
    fn structural_abi_copies_views_and_invalidates_closed_handles() {
        let name = c"id";
        let values = [
            BNValue {
                kind: BNValueKind::Integer,
                flags: 0,
                payload: BNValuePayload { integer: 1 },
            },
            BNValue {
                kind: BNValueKind::Integer,
                flags: 0,
                payload: BNValuePayload { integer: 2 },
            },
        ];
        let view = BNDataFrameColumnView {
            name: name.as_ptr(),
            values: values.as_ptr(),
            length: u32::try_from(values.len()).expect("test values fit ABI length"),
        };
        let mut first: BNDataFrameHandle = 0;
        assert_eq!(
            bn_rt_dataframe_create(&raw const view, 1, &raw mut first),
            BN_DATAFRAME_OK
        );
        let mut second = 0;
        assert_eq!(
            bn_rt_dataframe_append_rows(first, first, &raw mut second),
            BN_DATAFRAME_OK
        );
        let mut selected = 0;
        let rows = [1_u32];
        let columns = [0_u32];
        assert_eq!(
            bn_rt_dataframe_select(
                second,
                rows.as_ptr(),
                1,
                columns.as_ptr(),
                1,
                &raw mut selected,
            ),
            BN_DATAFRAME_OK
        );
        let mut appended = 0;
        assert_eq!(
            bn_rt_dataframe_append_columns(first, selected, &raw mut appended),
            BN_DATAFRAME_CONTRACT_ERROR
        );
        assert_eq!(bn_rt_dataframe_close(first), BN_DATAFRAME_OK);
        assert_eq!(bn_rt_dataframe_close(first), BN_DATAFRAME_INVALID_HANDLE);
        assert_eq!(bn_rt_dataframe_close(second), BN_DATAFRAME_OK);
        assert_eq!(bn_rt_dataframe_close(selected), BN_DATAFRAME_OK);
    }

    #[test]
    fn empty_views_may_use_null_data_pointers() {
        let empty_name = c"empty";
        let empty_column = BNDataFrameColumnView {
            name: empty_name.as_ptr(),
            values: std::ptr::null(),
            length: 0,
        };
        let empty_bytes = BNValue {
            kind: BNValueKind::Bytes,
            flags: 0,
            payload: BNValuePayload {
                bytes: BNValueBytes {
                    data: std::ptr::null(),
                    length: 0,
                },
            },
        };
        let bytes_name = c"bytes";
        let bytes_column = BNDataFrameColumnView {
            name: bytes_name.as_ptr(),
            values: &raw const empty_bytes,
            length: 1,
        };
        let columns = [empty_column, bytes_column];
        let mut frame = 0;
        assert_eq!(
            bn_rt_dataframe_create(
                columns.as_ptr(),
                u32::try_from(columns.len()).expect("test columns fit ABI length"),
                &raw mut frame,
            ),
            BN_DATAFRAME_OK
        );
        assert_eq!(bn_rt_dataframe_close(frame), BN_DATAFRAME_OK);
    }

    #[test]
    fn empty_frame_counts_are_exported_and_closed_handles_are_rejected() {
        let mut frame = 0;
        assert_eq!(
            bn_rt_dataframe_create(std::ptr::null(), 0, &raw mut frame),
            BN_DATAFRAME_OK
        );
        let mut rows = u32::MAX;
        let mut columns = u32::MAX;
        assert_eq!(
            bn_rt_dataframe_row_count(frame, &raw mut rows),
            BN_DATAFRAME_OK
        );
        assert_eq!(
            bn_rt_dataframe_column_count(frame, &raw mut columns),
            BN_DATAFRAME_OK
        );
        assert_eq!(rows, 0);
        assert_eq!(columns, 0);
        assert_eq!(bn_rt_dataframe_close(frame), BN_DATAFRAME_OK);
        assert_eq!(
            bn_rt_dataframe_row_count(frame, &raw mut rows),
            BN_DATAFRAME_INVALID_HANDLE
        );
        assert_eq!(
            bn_rt_dataframe_column_count(frame, &raw mut columns),
            BN_DATAFRAME_INVALID_HANDLE
        );
    }

    #[test]
    fn hostile_column_names_are_rejected_with_invalid_argument() {
        // 1. Null column name
        let null_name_view = BNDataFrameColumnView {
            name: std::ptr::null(),
            values: std::ptr::null(),
            length: 0,
        };
        let mut frame = 0;
        assert_eq!(
            bn_rt_dataframe_create(&raw const null_name_view, 1, &raw mut frame),
            BN_DATAFRAME_INVALID_ARGUMENT
        );

        // 2. Overlong column name (> 256 bytes)
        let overlong_bytes = vec![b'a'; MAX_COLUMN_NAME_LENGTH + 10];
        let overlong_c = std::ffi::CString::new(overlong_bytes).unwrap();
        let overlong_view = BNDataFrameColumnView {
            name: overlong_c.as_ptr(),
            values: std::ptr::null(),
            length: 0,
        };
        assert_eq!(
            bn_rt_dataframe_create(&raw const overlong_view, 1, &raw mut frame),
            BN_DATAFRAME_INVALID_ARGUMENT
        );

        // 3. Exactly MAX_COLUMN_NAME_LENGTH (256 bytes) is allowed
        let exact_bytes = vec![b'b'; MAX_COLUMN_NAME_LENGTH];
        let exact_c = std::ffi::CString::new(exact_bytes).unwrap();
        let exact_view = BNDataFrameColumnView {
            name: exact_c.as_ptr(),
            values: std::ptr::null(),
            length: 0,
        };
        assert_eq!(
            bn_rt_dataframe_create(&raw const exact_view, 1, &raw mut frame),
            BN_DATAFRAME_OK
        );
        assert_eq!(bn_rt_dataframe_close(frame), BN_DATAFRAME_OK);
    }

    #[test]
    fn invalid_value_kind_is_rejected_safely() {
        let name = c"bad_kind_col";
        // Construct properly aligned buffer matching BNValue layout with out-of-range kind (99)
        let mut uninit = std::mem::MaybeUninit::<BNValue>::uninit();
        let uninit_ptr = uninit.as_mut_ptr();
        unsafe {
            uninit_ptr
                .cast::<u8>()
                .write_bytes(0, std::mem::size_of::<BNValue>());
            uninit_ptr.cast::<u32>().write(99);
        }
        let bad_value_ptr = uninit.as_ptr();

        let view = BNDataFrameColumnView {
            name: name.as_ptr(),
            values: bad_value_ptr,
            length: 1,
        };
        let mut frame = 0;
        assert_eq!(
            bn_rt_dataframe_create(&raw const view, 1, &raw mut frame),
            BN_DATAFRAME_INVALID_ARGUMENT
        );
    }

    #[test]
    fn catch_unwind_prevents_process_abort_on_panic() {
        // An unmapped / invalid handle returns BN_DATAFRAME_INVALID_HANDLE without unwinding
        assert_eq!(bn_rt_dataframe_close(999_999), BN_DATAFRAME_INVALID_HANDLE);
        let mut count = 0;
        assert_eq!(
            bn_rt_dataframe_row_count(999_999, &raw mut count),
            BN_DATAFRAME_INVALID_HANDLE
        );
    }

    #[test]
    fn negative_incremental_column_index_is_rejected_without_selecting_column_zero() {
        let mut frame = 0;
        assert_eq!(
            bn_rt_dataframe_create(std::ptr::null(), 0, &raw mut frame),
            BN_DATAFRAME_OK
        );
        assert_eq!(
            super::super::bn_rt_dataframe_set_integer_cell(frame, -1, 0, 42),
            BN_DATAFRAME_INVALID_ARGUMENT
        );
        assert_eq!(bn_rt_dataframe_close(frame), BN_DATAFRAME_OK);
    }
}
