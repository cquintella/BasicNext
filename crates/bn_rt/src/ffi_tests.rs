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
}
