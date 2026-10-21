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
    // SAFETY: the buffer is local to this test; only the bytes this write fills are read.
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
