use super::*;
use crate::file_abi::{bn_rt_file_close, bn_rt_file_open};
use std::ffi::CString;

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
        let name = CString::new(path.to_str().unwrap()).unwrap();
        let mut file = 0;
        assert_eq!(bn_rt_file_open(name.as_ptr(), 0, &mut file), 0);
        let mut frame = 0;
        let status = bn_rt_dataframe_read_csv(file, 1, c",".as_ptr(), &mut frame);
        assert_eq!(bn_rt_file_close(file), 0);
        std::fs::remove_file(path).unwrap();
        assert_eq!(status, BN_DATAFRAME_CONTRACT_ERROR, "{content}");
        assert_eq!(frame, 0);
    }
}
