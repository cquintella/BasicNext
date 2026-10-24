// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[cfg(test)]
mod tests {
    use crate::ffi::*;

    #[test]
    fn test_record_lifecycle() {
        let h = allocate_record(TYPE_INT32, CValuePayload::Int32(42));
        let rec = get_record(h).expect("get record");
        assert_eq!(rec.type_code, TYPE_INT32);
        match rec.payload {
            CValuePayload::Int32(val) => assert_eq!(val, 42),
            _ => panic!("unexpected payload"),
        }
        release_record(h);
        assert!(get_record(h).is_none());
    }

    #[test]
    fn test_libc_strlen_and_labs() {
        let libc_name = if cfg!(target_os = "windows") {
            "ucrtbase"
        } else if cfg!(target_os = "macos") {
            "libSystem.B.dylib"
        } else {
            "libc.so.6"
        };

        let arg_h = allocate_record(TYPE_STRING, CValuePayload::String("hello world".into()));
        let res_h = allocate_record(TYPE_SIZE_T, CValuePayload::SizeT(0));

        let call_res = call_c_function(libc_name, "strlen", &[], &[arg_h], res_h);
        assert!(call_res.is_ok(), "strlen call failed: {:?}", call_res.err());

        let res_rec = get_record(res_h).expect("read result");
        match res_rec.payload {
            CValuePayload::SizeT(len) => assert_eq!(len, 11),
            _ => panic!("unexpected payload for strlen"),
        }

        release_record(arg_h);
        release_record(res_h);
    }

    #[test]
    fn test_handle_result_reuses_and_frees_child_handle() {
        let hr = allocate_record(
            TYPE_HANDLE_RESULT,
            CValuePayload::HandleResult(Some(std::ptr::without_provenance_mut(0x1234))),
        );
        let child1 = handle_result_child_handle(hr).expect("first child handle");
        let child2 = handle_result_child_handle(hr).expect("second child handle");
        assert_eq!(
            child1, child2,
            "HandleResult.Value() must reuse child handle"
        );
        assert!(get_record(child1).is_some());

        release_record(hr);
        assert!(get_record(hr).is_none());
        assert!(
            get_record(child1).is_none(),
            "releasing HandleResult must release its child Handle"
        );
    }

    #[test]
    fn test_array_read_write_checked_mul_overflow_rejected() {
        let mut backing = [0u8; 16];
        let arr = allocate_record(
            TYPE_ARRAY,
            CValuePayload::Array {
                data: backing.as_mut_ptr(),
                element_size: 8,
                count: usize::MAX,
                region_arc_id: None,
            },
        );
        let dummy_ptr = std::ptr::without_provenance_mut(0x1000);
        assert!(
            array_read_from(arr, dummy_ptr).is_err(),
            "overflowing count * element_size must fail before dereferencing pointer"
        );
        assert!(
            array_write_to(arr, dummy_ptr).is_err(),
            "overflowing count * element_size must fail before dereferencing pointer"
        );
        release_record(arr);
    }

    #[test]
    fn test_concurrent_resolve_library_and_ffi_paths_key() {
        let libc_name = if cfg!(target_os = "windows") {
            "ucrtbase"
        } else if cfg!(target_os = "macos") {
            "libSystem.B.dylib"
        } else {
            "libc.so.6"
        };

        std::thread::scope(|s| {
            for i in 0..8 {
                s.spawn(move || {
                    let paths = if i % 2 == 0 {
                        vec![]
                    } else {
                        vec![std::path::PathBuf::from("/nonexistent_ffi_dir_test")]
                    };
                    let lib = crate::ffi_loader::resolve_library(libc_name, &paths)
                        .expect("resolve system libc");
                    let sym = crate::ffi_loader::resolve_symbol(libc_name, "strlen", &lib)
                        .expect("resolve strlen");
                    assert_ne!(sym, 0);
                });
            }
        });
    }

    #[test]
    fn ffi_type_codes_match_llvm_abi_contract() {
        let table = [
            (TYPE_INT8, 1),
            (TYPE_UINT8, 2),
            (TYPE_INT16, 3),
            (TYPE_UINT16, 4),
            (TYPE_INT32, 5),
            (TYPE_UINT32, 6),
            (TYPE_INT64, 7),
            (TYPE_UINT64, 8),
            (TYPE_INT, 9),
            (TYPE_UINT, 10),
            (TYPE_LONG, 11),
            (TYPE_ULONG, 12),
            (TYPE_SIZE_T, 13),
            (TYPE_FLOAT, 14),
            (TYPE_DOUBLE, 15),
            (TYPE_BOOL, 16),
            (TYPE_STRING, 17),
            (TYPE_VOID, 18),
            (TYPE_HANDLE, 19),
            (TYPE_HANDLE_RESULT, 20),
            (TYPE_BYTE_ARRAY, 21),
            (TYPE_INT8_ARRAY, 22),
            (TYPE_INT16_ARRAY, 23),
            (TYPE_INT32_ARRAY, 24),
            (TYPE_INT64_ARRAY, 25),
            (TYPE_UINT16_ARRAY, 26),
            (TYPE_UINT32_ARRAY, 27),
            (TYPE_UINT64_ARRAY, 28),
            (TYPE_FLOAT_ARRAY, 29),
            (TYPE_DOUBLE_ARRAY, 30),
            (TYPE_ARRAY, 31),
            (TYPE_ARGS, 32),
        ];
        for (actual, expected) in table {
            assert_eq!(actual, expected);
        }
    }
}
