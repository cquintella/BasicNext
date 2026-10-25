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
        let libc_name = system_c_library_name();

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
        let baseline = active_record_count();
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
        assert!(active_record_count() >= baseline + 2);

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
        let dummy_ptr = std::ptr::without_provenance_mut(0x1000);
        for elem_size in [2usize, 4, 8] {
            for bad_count in [usize::MAX, (usize::MAX / elem_size) + 1] {
                let arr = allocate_record(
                    TYPE_ARRAY,
                    CValuePayload::Array {
                        data: backing.as_mut_ptr(),
                        element_size: elem_size,
                        count: bad_count,
                        region_arc_id: None,
                    },
                );
                assert!(
                    array_read_from(arr, dummy_ptr).is_err(),
                    "overflowing count * element_size ({elem_size} * {bad_count}) must fail"
                );
                assert!(
                    array_write_to(arr, dummy_ptr).is_err(),
                    "overflowing count * element_size ({elem_size} * {bad_count}) must fail"
                );
                release_record(arr);
            }
        }

        // Negative or overflowing counts passed to `bn_rt_ffi_set_array` transition to Void.
        let h = allocate_record(TYPE_INT32_ARRAY, CValuePayload::Void);
        crate::ffi_abi::bn_rt_ffi_set_array(h, std::ptr::null_mut(), 4, (-1isize).cast_unsigned());
        assert!(matches!(
            get_record(h).expect("record").payload,
            CValuePayload::Void
        ));
        assert!(array_read_from(h, dummy_ptr).is_err());
        release_record(h);
    }

    #[test]
    fn test_concurrent_resolve_library_and_ffi_paths_key() {
        let libc_name = system_c_library_name();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        std::thread::scope(|s| {
            for i in 0..8 {
                let b = barrier.clone();
                s.spawn(move || {
                    b.wait();
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

                    let rec = allocate_record(TYPE_INT32, CValuePayload::Int32(i));
                    assert!(get_record(rec).is_some());
                    release_record(rec);
                    assert!(get_record(rec).is_none());
                });
            }
        });
    }

    #[test]
    fn test_c_long_ulong_size_t_platform_width_bounds() {
        // Test both 32-bit (LLP64 Windows) and 64-bit (LP64 Unix) width contracts on every host OS.
        for val in [i64::from(i32::MAX) + 1, i64::from(i32::MIN) - 1] {
            let err = validate_c_long_for_width(val, 4)
                .expect_err("4-byte c_long (LLP64) must reject out-of-i32-range value");
            assert_eq!(err.code, ERROR_VALUE_OUT_OF_RANGE);
            assert_eq!(
                validate_c_long_for_width(val, 8).expect("8-byte c_long (LP64) must accept"),
                val
            );
        }
        for ok_val in [i64::from(i32::MIN), 0, i64::from(i32::MAX)] {
            assert_eq!(validate_c_long_for_width(ok_val, 4).unwrap(), ok_val);
            assert_eq!(validate_c_long_for_width(ok_val, 8).unwrap(), ok_val);
        }

        let u_overflow = u64::from(u32::MAX) + 1;
        assert_eq!(
            validate_c_ulong_for_width(u_overflow, 4)
                .expect_err("4-byte c_ulong must reject u32::MAX + 1")
                .code,
            ERROR_VALUE_OUT_OF_RANGE
        );
        assert_eq!(
            validate_c_ulong_for_width(u_overflow, 8).unwrap(),
            u_overflow
        );
        assert_eq!(
            validate_size_t_for_width(u_overflow, 4)
                .expect_err("4-byte size_t must reject u32::MAX + 1")
                .code,
            ERROR_VALUE_OUT_OF_RANGE
        );
        assert_eq!(
            validate_size_t_for_width(u_overflow, 8).unwrap(),
            u_overflow
        );

        // Verify OS filename and system C library resolution across all target OS names on every host.
        assert_eq!(
            format_library_filename_for_os("mylib", "windows"),
            "mylib.dll"
        );
        assert_eq!(
            format_library_filename_for_os("mylib", "macos"),
            "libmylib.dylib"
        );
        assert_eq!(
            format_library_filename_for_os("mylib", "linux"),
            "libmylib.so"
        );
        assert_eq!(
            format_library_filename_for_os("custom.so.1", "windows"),
            "custom.so.1"
        );
        assert_eq!(system_c_library_name_for_os("windows"), "ucrtbase");
        assert_eq!(system_c_library_name_for_os("macos"), "libSystem.B.dylib");
        assert_eq!(system_c_library_name_for_os("linux"), "libc.so.6");

        let libc_name = system_c_library_name();
        let res_h = allocate_record(TYPE_LONG, CValuePayload::Long(0));
        for val in [i64::from(i32::MAX) + 1, i64::from(i32::MIN) - 1] {
            let arg_h = allocate_record(TYPE_LONG, CValuePayload::Long(val));
            let res = call_c_function(libc_name, "labs", &[], &[arg_h], res_h);
            let expected_ok =
                validate_c_long_for_width(val, std::mem::size_of::<libc::c_long>()).is_ok();
            assert_eq!(res.is_ok(), expected_ok);
            if let Err(err) = res {
                assert_eq!(err.code, ERROR_VALUE_OUT_OF_RANGE);
            }
            release_record(arg_h);
        }
        release_record(res_h);
    }

    #[test]
    fn test_ffi_abi_null_and_invalid_utf8_rejected() {
        let libc_name = system_c_library_name();

        let null_str_h = crate::ffi_abi::bn_rt_ffi_new_record(TYPE_STRING);
        crate::ffi_abi::bn_rt_ffi_set_string(null_str_h, std::ptr::null());
        assert!(matches!(
            get_record(null_str_h).expect("record").payload,
            CValuePayload::Void
        ));
        let res_h = allocate_record(TYPE_SIZE_T, CValuePayload::SizeT(0));
        let err = call_c_function(libc_name, "strlen", &[], &[null_str_h], res_h)
            .expect_err("null string record must be rejected");
        assert_eq!(err.code, ERROR_INVALID_SIGNATURE);

        let bad_utf8 = [0xFFu8, 0x00u8];
        crate::ffi_abi::bn_rt_ffi_set_string(null_str_h, bad_utf8.as_ptr().cast());
        assert!(matches!(
            get_record(null_str_h).expect("record").payload,
            CValuePayload::Void
        ));
        let err2 = call_c_function(libc_name, "strlen", &[], &[null_str_h], res_h)
            .expect_err("invalid UTF-8 string record must be rejected");
        assert_eq!(err2.code, ERROR_INVALID_SIGNATURE);

        release_record(null_str_h);
        release_record(res_h);
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
