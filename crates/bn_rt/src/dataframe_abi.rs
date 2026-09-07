//! Stable C ABI for the structural `BNData.DataFrame` operations.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::dataframe::{
    DataFrameColumn, DataFrameResource, append_columns, append_rows, select_dataframe,
};
use super::dispatch_abi::BNValue;

pub type BNDataFrameHandle = u64;
pub type BNDataFrameStatus = u32;

pub const BN_DATAFRAME_OK: BNDataFrameStatus = 0;
pub const BN_DATAFRAME_INVALID_ARGUMENT: BNDataFrameStatus = 1;
pub const BN_DATAFRAME_INVALID_HANDLE: BNDataFrameStatus = 2;
pub const BN_DATAFRAME_CONTRACT_ERROR: BNDataFrameStatus = 3;

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

fn with_frames<T>(operation: impl FnOnce(&mut HashMap<BNDataFrameHandle, Frame>) -> T) -> T {
    operation(
        &mut registry()
            .frames
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
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
            return BN_DATAFRAME_INVALID_ARGUMENT;
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
            return BN_DATAFRAME_INVALID_ARGUMENT;
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
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
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
            return BN_DATAFRAME_INVALID_ARGUMENT;
        }
        let result = with_frames(|frames| {
            let (Some(left), Some(right)) = (frames.get(&left), frames.get(&right)) else {
                return Err(BN_DATAFRAME_INVALID_HANDLE);
            };
            append_rows(left, right, is_missing, same_stored_type)
                .map_err(|_| BN_DATAFRAME_CONTRACT_ERROR)
        });
        let frame = match result {
            Ok(frame) => frame,
            Err(status) => return status,
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
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
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
            return BN_DATAFRAME_INVALID_ARGUMENT;
        }
        let result = with_frames(|frames| {
            let (Some(left), Some(right)) = (frames.get(&left), frames.get(&right)) else {
                return Err(BN_DATAFRAME_INVALID_HANDLE);
            };
            append_columns(left, right).map_err(|_| BN_DATAFRAME_CONTRACT_ERROR)
        });
        let frame = match result {
            Ok(frame) => frame,
            Err(status) => return status,
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
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
}

/// Returns the number of rows in a frame.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_row_count(
    frame: BNDataFrameHandle,
    out_count: *mut u32,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_count.is_null() {
            return BN_DATAFRAME_INVALID_ARGUMENT;
        }
        let Some(count) = with_frames(|frames| {
            frames.get(&frame).map(|resource| {
                resource
                    .columns
                    .first()
                    .map_or(0, |column| column.values.len())
            })
        }) else {
            return BN_DATAFRAME_INVALID_HANDLE;
        };
        let Ok(count) = u32::try_from(count) else {
            return BN_DATAFRAME_CONTRACT_ERROR;
        };
        unsafe {
            *out_count = count;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
}

/// Returns the number of columns in a frame.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_column_count(
    frame: BNDataFrameHandle,
    out_count: *mut u32,
) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if out_count.is_null() {
            return BN_DATAFRAME_INVALID_ARGUMENT;
        }
        let Some(count) =
            with_frames(|frames| frames.get(&frame).map(|resource| resource.columns.len()))
        else {
            return BN_DATAFRAME_INVALID_HANDLE;
        };
        let Ok(count) = u32::try_from(count) else {
            return BN_DATAFRAME_CONTRACT_ERROR;
        };
        unsafe {
            *out_count = count;
        }
        BN_DATAFRAME_OK
    }))
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
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
            return BN_DATAFRAME_INVALID_ARGUMENT;
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
                return Err(BN_DATAFRAME_INVALID_HANDLE);
            };
            select_dataframe(source, &row_indices, &column_indices)
                .map_err(|_| BN_DATAFRAME_CONTRACT_ERROR)
        });
        let result = match result {
            Ok(result) => result,
            Err(status) => return status,
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
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
}

/// Closes a frame handle. Closing twice returns `INVALID_HANDLE`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_close(frame: BNDataFrameHandle) -> BNDataFrameStatus {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_frames(|frames| {
            frames
                .remove(&frame)
                .map_or(BN_DATAFRAME_INVALID_HANDLE, |_| BN_DATAFRAME_OK)
        })
    }))
    .unwrap_or(BN_DATAFRAME_CONTRACT_ERROR)
}

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
}
