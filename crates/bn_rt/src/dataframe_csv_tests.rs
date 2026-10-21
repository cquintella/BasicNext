use super::*;
use crate::file_abi::{bn_rt_file_close, bn_rt_file_open};
#[test]
fn headerless_and_empty_csv_preserve_existing_labels_and_dimensions() {
    let frame =
        crate::dataframe::frame_from_csv_rows(vec![vec!["é".into(), String::new()]], false, |s| s)
            .unwrap();
    assert_eq!(frame.columns[0].name, "Column1");
    assert_eq!(frame.columns[0].values, ["é"]);
    assert_eq!(frame.columns[1].name, "Column2");
    assert_eq!(frame.columns[1].values, [""]);
    for header in [true, false] {
        assert!(
            crate::dataframe::frame_from_csv_rows(Vec::new(), header, |s| s)
                .unwrap()
                .columns
                .is_empty()
        );
    }
}

#[test]
#[allow(clippy::borrow_as_ptr)]
fn csv_rejects_ragged_rows_and_duplicate_headers() {
    for (index, content) in ["a,b\n1\n", "a,b\n1,2,3\n", "a,a\n1,2\n"]
        .iter()
        .enumerate()
    {
        let path = std::env::temp_dir().join(format!("bn-csv-{}-{index}", std::process::id()));
        std::fs::write(&path, content).unwrap();
        let name = crate::string::c_string_from_bytes(path.to_str().unwrap().as_bytes());
        let mut file = 0;
        assert_eq!(bn_rt_file_open(name, 0, &mut file), 0);
        let mut frame = 0;
        let sep = crate::string::c_string_from_bytes(b",");
        let status = bn_rt_dataframe_read_csv(file, 1, sep, &mut frame);
        crate::string::bn_rt_str_release(sep);
        assert_eq!(bn_rt_file_close(file), 0);
        std::fs::remove_file(path).unwrap();
        assert_eq!(status, BN_DATAFRAME_INVALID_FORMAT, "{content}");
        assert_eq!(frame, 0);
    }
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

#[test]
fn copy_integer_rejects_64bit_overflow_with_invalid_format() {
    let mut frame = 0;
    assert_eq!(
        bn_rt_dataframe_create(std::ptr::null(), 0, &raw mut frame),
        BN_DATAFRAME_OK
    );
    let col_name = crate::string::c_string_from_bytes(b"big");
    let col_idx = super::add_integer_column_storage(frame, "big", 1).expect("add column");
    assert_eq!(
        super::super::bn_rt_dataframe_set_integer_cell(frame, col_idx as i32, 0, 3_000_000_000_i64),
        BN_DATAFRAME_OK
    );
    let mut target = [0_i32; 1];
    assert_eq!(
        super::bn_rt_dataframe_copy_integer(frame, col_name, target.as_mut_ptr(), 1),
        super::BN_DATAFRAME_INVALID_FORMAT
    );
    assert_eq!(target[0], 0);
    assert_eq!(crate::string::bn_rt_str_release(col_name), 0);
    assert_eq!(bn_rt_dataframe_close(frame), BN_DATAFRAME_OK);
}
