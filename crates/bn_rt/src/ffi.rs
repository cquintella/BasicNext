#![allow(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::ptr_cast_constness)]

//! Foreign Function Interface (FFI) execution engine for `BNFfi`.
//!
//! Provides dynamic library loading, symbol resolution, and typed C function
//! invocation using libffi and libloading. Interpreter (`bn_lib_ffi`) and
//! compiled native binaries (`bn_rt::ffi_abi`) share this implementation.

use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use libffi::middle::{Arg, Cif, CodePtr, Type};

// Error codes defined in language/0.6/bnffi.md.
pub const ERROR_INVALID_ARGUMENT: i32 = 1;
pub const ERROR_LIBRARY_NOT_FOUND: i32 = 2;
pub const ERROR_LIBRARY_LOAD_FAILED: i32 = 3;
pub const ERROR_SYMBOL_NOT_FOUND: i32 = 4;
pub const ERROR_INVALID_SIGNATURE: i32 = 5;
pub const ERROR_VALUE_OUT_OF_RANGE: i32 = 6;
pub const ERROR_INVALID_STRING: i32 = 7;
pub const ERROR_INVALID_UTF8: i32 = 8;

// Type codes for C.* classes.
pub const TYPE_INT8: u32 = 1;
pub const TYPE_UINT8: u32 = 2;
pub const TYPE_INT16: u32 = 3;
pub const TYPE_UINT16: u32 = 4;
pub const TYPE_INT32: u32 = 5;
pub const TYPE_UINT32: u32 = 6;
pub const TYPE_INT64: u32 = 7;
pub const TYPE_UINT64: u32 = 8;
pub const TYPE_INT: u32 = 9;
pub const TYPE_UINT: u32 = 10;
pub const TYPE_LONG: u32 = 11;
pub const TYPE_ULONG: u32 = 12;
pub const TYPE_SIZE_T: u32 = 13;
pub const TYPE_FLOAT: u32 = 14;
pub const TYPE_DOUBLE: u32 = 15;
pub const TYPE_BOOL: u32 = 16;
pub const TYPE_STRING: u32 = 17;
pub const TYPE_VOID: u32 = 18;
pub const TYPE_HANDLE: u32 = 19;
pub const TYPE_HANDLE_RESULT: u32 = 20;

// Array type codes.
pub const TYPE_BYTE_ARRAY: u32 = 21;
pub const TYPE_INT8_ARRAY: u32 = 22;
pub const TYPE_INT16_ARRAY: u32 = 23;
pub const TYPE_INT32_ARRAY: u32 = 24;
pub const TYPE_INT64_ARRAY: u32 = 25;
pub const TYPE_UINT16_ARRAY: u32 = 26;
pub const TYPE_UINT32_ARRAY: u32 = 27;
pub const TYPE_UINT64_ARRAY: u32 = 28;
pub const TYPE_FLOAT_ARRAY: u32 = 29;
pub const TYPE_DOUBLE_ARRAY: u32 = 30;
pub const TYPE_ARRAY: u32 = 31;
pub const TYPE_ARGS: u32 = 32;

#[derive(Debug, Clone)]
pub struct FfiError {
    pub code: i32,
    pub message: String,
    pub cause: Option<String>,
}

impl FfiError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
        }
    }

    pub fn with_cause(code: i32, message: impl Into<String>, cause: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: Some(cause.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CValuePayload {
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Int(i32),
    UInt(u32),
    Long(i64),
    ULong(u64),
    SizeT(u64),
    Float(f32),
    Double(f64),
    Bool(bool),
    String(String),
    Void,
    Handle(*mut c_void),
    HandleResult(Option<*mut c_void>),
    Args(Vec<u64>),
    Array {
        data: *mut u8,
        element_size: usize,
        count: usize,
    },
}

// Safety: pointers in CValuePayload represent memory buffers whose lifetimes are
// managed by the Basic Next caller or the foreign library.
#[allow(unsafe_code)]
unsafe impl Send for CValuePayload {}
#[allow(unsafe_code)]
unsafe impl Sync for CValuePayload {}

#[derive(Debug, Clone)]
pub struct Record {
    pub type_code: u32,
    pub payload: CValuePayload,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static RECORDS: OnceLock<Mutex<HashMap<u64, Record>>> = OnceLock::new();

fn records_map() -> &'static Mutex<HashMap<u64, Record>> {
    RECORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn allocate_record(type_code: u32, payload: CValuePayload) -> u64 {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    let mut map = records_map().lock().expect("lock records");
    map.insert(handle, Record { type_code, payload });
    handle
}

pub fn release_record(handle: u64) {
    let mut map = records_map().lock().expect("lock records");
    map.remove(&handle);
}

pub fn get_record(handle: u64) -> Option<Record> {
    let map = records_map().lock().expect("lock records");
    map.get(&handle).cloned()
}

pub fn update_record_payload(handle: u64, payload: CValuePayload) -> bool {
    let mut map = records_map().lock().expect("lock records");
    if let Some(rec) = map.get_mut(&handle) {
        rec.payload = match (rec.type_code, payload) {
            (TYPE_SIZE_T, CValuePayload::UInt64(v)) => CValuePayload::SizeT(v),
            (TYPE_LONG, CValuePayload::Int64(v)) => CValuePayload::Long(v),
            (TYPE_ULONG, CValuePayload::UInt64(v)) => CValuePayload::ULong(v),
            (TYPE_INT, CValuePayload::Int32(v)) => CValuePayload::Int(v),
            (TYPE_UINT, CValuePayload::UInt32(v)) => CValuePayload::UInt(v),
            (_, p) => p,
        };
        true
    } else {
        false
    }
}

pub use super::ffi_loader::{resolve_library, resolve_symbol};

fn type_code_to_libffi_type(code: u32) -> Result<Type, FfiError> {
    match code {
        TYPE_INT8 => Ok(Type::i8()),
        TYPE_UINT8 => Ok(Type::u8()),
        TYPE_INT16 => Ok(Type::i16()),
        TYPE_UINT16 => Ok(Type::u16()),
        TYPE_INT32 | TYPE_INT => Ok(Type::i32()),
        TYPE_UINT32 | TYPE_UINT => Ok(Type::u32()),
        TYPE_INT64 => Ok(Type::i64()),
        TYPE_UINT64 => Ok(Type::u64()),
        TYPE_LONG => Ok(Type::c_long()),
        TYPE_ULONG => Ok(Type::c_ulong()),
        TYPE_SIZE_T => Ok(Type::usize()),
        TYPE_FLOAT => Ok(Type::f32()),
        TYPE_DOUBLE => Ok(Type::f64()),
        TYPE_BOOL => Ok(Type::u8()),
        TYPE_STRING => Ok(Type::pointer()),
        TYPE_VOID => Ok(Type::void()),
        TYPE_HANDLE | TYPE_HANDLE_RESULT => Ok(Type::pointer()),
        TYPE_BYTE_ARRAY | TYPE_INT8_ARRAY | TYPE_INT16_ARRAY | TYPE_INT32_ARRAY
        | TYPE_INT64_ARRAY | TYPE_UINT16_ARRAY | TYPE_UINT32_ARRAY | TYPE_UINT64_ARRAY
        | TYPE_FLOAT_ARRAY | TYPE_DOUBLE_ARRAY | TYPE_ARRAY => Ok(Type::pointer()),
        _ => Err(FfiError::new(
            ERROR_INVALID_SIGNATURE,
            format!("unknown type code: {code}"),
        )),
    }
}

/// Executes a native C call using libffi.
///
/// Crucial invariant: No mutex lock is held while executing the foreign call.
#[allow(unsafe_code)]
pub fn call_c_function(
    library_name: &str,
    symbol_name: &str,
    ffi_paths: &[PathBuf],
    arg_handles: &[u64],
    result_handle: u64,
) -> Result<(), FfiError> {
    let library = resolve_library(library_name, ffi_paths)?;
    let sym_ptr = resolve_symbol(library_name, symbol_name, &library)?;

    let mut arg_records = Vec::with_capacity(arg_handles.len());
    for &h in arg_handles {
        let rec = get_record(h).ok_or_else(|| {
            FfiError::new(
                ERROR_INVALID_ARGUMENT,
                format!("invalid argument handle: {h}"),
            )
        })?;
        if rec.type_code == TYPE_VOID || rec.type_code == TYPE_HANDLE_RESULT {
            return Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                format!(
                    "type code {} cannot be passed as an argument",
                    rec.type_code
                ),
            ));
        }
        arg_records.push(rec);
    }

    let result_rec = get_record(result_handle).ok_or_else(|| {
        FfiError::new(
            ERROR_INVALID_ARGUMENT,
            format!("invalid result handle: {result_handle}"),
        )
    })?;

    if result_rec.type_code == TYPE_STRING
        || result_rec.type_code == TYPE_HANDLE
        || (result_rec.type_code >= TYPE_BYTE_ARRAY && result_rec.type_code <= TYPE_DOUBLE_ARRAY)
    {
        return Err(FfiError::new(
            ERROR_INVALID_SIGNATURE,
            format!(
                "type code {} cannot be used as a return type",
                result_rec.type_code
            ),
        ));
    }

    let mut arg_types = Vec::with_capacity(arg_records.len());
    for rec in &arg_records {
        arg_types.push(type_code_to_libffi_type(rec.type_code)?);
    }
    let ret_type = type_code_to_libffi_type(result_rec.type_code)?;
    let cif = Cif::new(arg_types, ret_type);

    // Prepare argument storage and keep alive across the call.
    let mut i8_vals = Vec::new();
    let mut u8_vals = Vec::new();
    let mut i16_vals = Vec::new();
    let mut u16_vals = Vec::new();
    let mut i32_vals = Vec::new();
    let mut u32_vals = Vec::new();
    let mut i64_vals = Vec::new();
    let mut u64_vals = Vec::new();
    let mut long_vals = Vec::new();
    let mut ulong_vals = Vec::new();
    let mut usize_vals = Vec::new();
    let mut f32_vals = Vec::new();
    let mut f64_vals = Vec::new();
    let mut bool_vals = Vec::new();
    let mut ptr_vals = Vec::new();
    let mut c_strings = Vec::new();

    for rec in &arg_records {
        match rec.type_code {
            TYPE_INT8 => {
                let v = match rec.payload {
                    CValuePayload::Int8(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Int8 payload",
                        ));
                    }
                };
                i8_vals.push(v);
            }
            TYPE_UINT8 => {
                let v = match rec.payload {
                    CValuePayload::UInt8(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected UInt8 payload",
                        ));
                    }
                };
                u8_vals.push(v);
            }
            TYPE_INT16 => {
                let v = match rec.payload {
                    CValuePayload::Int16(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Int16 payload",
                        ));
                    }
                };
                i16_vals.push(v);
            }
            TYPE_UINT16 => {
                let v = match rec.payload {
                    CValuePayload::UInt16(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected UInt16 payload",
                        ));
                    }
                };
                u16_vals.push(v);
            }
            TYPE_INT32 | TYPE_INT => {
                let v = match rec.payload {
                    CValuePayload::Int32(v) | CValuePayload::Int(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Int32/Int payload",
                        ));
                    }
                };
                i32_vals.push(v);
            }
            TYPE_UINT32 | TYPE_UINT => {
                let v = match rec.payload {
                    CValuePayload::UInt32(v) | CValuePayload::UInt(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected UInt32/UInt payload",
                        ));
                    }
                };
                u32_vals.push(v);
            }
            TYPE_INT64 => {
                let v = match rec.payload {
                    CValuePayload::Int64(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Int64 payload",
                        ));
                    }
                };
                i64_vals.push(v);
            }
            TYPE_UINT64 => {
                let v = match rec.payload {
                    CValuePayload::UInt64(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected UInt64 payload",
                        ));
                    }
                };
                u64_vals.push(v);
            }
            TYPE_LONG => {
                let v = match rec.payload {
                    CValuePayload::Long(v) | CValuePayload::Int64(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Long payload",
                        ));
                    }
                };
                #[cfg(target_os = "windows")]
                if v > i32::MAX as i64 || v < i32::MIN as i64 {
                    return Err(FfiError::new(
                        ERROR_VALUE_OUT_OF_RANGE,
                        format!("Long value {v} out of range for 32-bit C long"),
                    ));
                }
                long_vals.push(v as libc::c_long);
            }
            TYPE_ULONG => {
                let v = match rec.payload {
                    CValuePayload::ULong(v) | CValuePayload::UInt64(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected ULong payload",
                        ));
                    }
                };
                #[cfg(target_os = "windows")]
                if v > u32::MAX as u64 {
                    return Err(FfiError::new(
                        ERROR_VALUE_OUT_OF_RANGE,
                        format!("ULong value {v} out of range for 32-bit C unsigned long"),
                    ));
                }
                ulong_vals.push(v as libc::c_ulong);
            }
            TYPE_SIZE_T => {
                let v = match rec.payload {
                    CValuePayload::SizeT(v) | CValuePayload::UInt64(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected SizeT payload",
                        ));
                    }
                };
                usize_vals.push(v as usize);
            }
            TYPE_FLOAT => {
                let v = match rec.payload {
                    CValuePayload::Float(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Float payload",
                        ));
                    }
                };
                f32_vals.push(v);
            }
            TYPE_DOUBLE => {
                let v = match rec.payload {
                    CValuePayload::Double(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Double payload",
                        ));
                    }
                };
                f64_vals.push(v);
            }
            TYPE_BOOL => {
                let v = match rec.payload {
                    CValuePayload::Bool(v) => v,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Bool payload",
                        ));
                    }
                };
                bool_vals.push(if v { 1u8 } else { 0u8 });
            }
            TYPE_STRING => {
                let s = match &rec.payload {
                    CValuePayload::String(s) => s,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected String payload",
                        ));
                    }
                };
                if s.contains('\0') {
                    return Err(FfiError::new(
                        ERROR_INVALID_STRING,
                        "C.String contains embedded NUL",
                    ));
                }
                let cs = CString::new(s.as_str()).map_err(|_| {
                    FfiError::new(ERROR_INVALID_STRING, "failed to convert string to CString")
                })?;
                let ptr = cs.as_ptr();
                c_strings.push(cs);
                ptr_vals.push(ptr as *mut c_void);
            }
            TYPE_HANDLE => {
                let p = match rec.payload {
                    CValuePayload::Handle(p) => p,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Handle payload",
                        ));
                    }
                };
                ptr_vals.push(p);
            }
            TYPE_BYTE_ARRAY | TYPE_INT8_ARRAY | TYPE_INT16_ARRAY | TYPE_INT32_ARRAY
            | TYPE_INT64_ARRAY | TYPE_UINT16_ARRAY | TYPE_UINT32_ARRAY | TYPE_UINT64_ARRAY
            | TYPE_FLOAT_ARRAY | TYPE_DOUBLE_ARRAY | TYPE_ARRAY => {
                let ptr = match rec.payload {
                    CValuePayload::Array { data, .. } => data as *mut c_void,
                    _ => {
                        return Err(FfiError::new(
                            ERROR_INVALID_SIGNATURE,
                            "expected Array payload",
                        ));
                    }
                };
                ptr_vals.push(ptr);
            }
            _ => {
                return Err(FfiError::new(
                    ERROR_INVALID_SIGNATURE,
                    "unsupported argument type",
                ));
            }
        }
    }

    let mut i8_idx = 0;
    let mut u8_idx = 0;
    let mut i16_idx = 0;
    let mut u16_idx = 0;
    let mut i32_idx = 0;
    let mut u32_idx = 0;
    let mut i64_idx = 0;
    let mut u64_idx = 0;
    let mut long_idx = 0;
    let mut ulong_idx = 0;
    let mut usize_idx = 0;
    let mut f32_idx = 0;
    let mut f64_idx = 0;
    let mut bool_idx = 0;
    let mut ptr_idx = 0;

    let mut libffi_args = Vec::with_capacity(arg_records.len());
    for rec in &arg_records {
        match rec.type_code {
            TYPE_INT8 => {
                libffi_args.push(Arg::new(&i8_vals[i8_idx]));
                i8_idx += 1;
            }
            TYPE_UINT8 => {
                libffi_args.push(Arg::new(&u8_vals[u8_idx]));
                u8_idx += 1;
            }
            TYPE_INT16 => {
                libffi_args.push(Arg::new(&i16_vals[i16_idx]));
                i16_idx += 1;
            }
            TYPE_UINT16 => {
                libffi_args.push(Arg::new(&u16_vals[u16_idx]));
                u16_idx += 1;
            }
            TYPE_INT32 | TYPE_INT => {
                libffi_args.push(Arg::new(&i32_vals[i32_idx]));
                i32_idx += 1;
            }
            TYPE_UINT32 | TYPE_UINT => {
                libffi_args.push(Arg::new(&u32_vals[u32_idx]));
                u32_idx += 1;
            }
            TYPE_INT64 => {
                libffi_args.push(Arg::new(&i64_vals[i64_idx]));
                i64_idx += 1;
            }
            TYPE_UINT64 => {
                libffi_args.push(Arg::new(&u64_vals[u64_idx]));
                u64_idx += 1;
            }
            TYPE_LONG => {
                libffi_args.push(Arg::new(&long_vals[long_idx]));
                long_idx += 1;
            }
            TYPE_ULONG => {
                libffi_args.push(Arg::new(&ulong_vals[ulong_idx]));
                ulong_idx += 1;
            }
            TYPE_SIZE_T => {
                libffi_args.push(Arg::new(&usize_vals[usize_idx]));
                usize_idx += 1;
            }
            TYPE_FLOAT => {
                libffi_args.push(Arg::new(&f32_vals[f32_idx]));
                f32_idx += 1;
            }
            TYPE_DOUBLE => {
                libffi_args.push(Arg::new(&f64_vals[f64_idx]));
                f64_idx += 1;
            }
            TYPE_BOOL => {
                libffi_args.push(Arg::new(&bool_vals[bool_idx]));
                bool_idx += 1;
            }
            TYPE_STRING | TYPE_HANDLE | TYPE_BYTE_ARRAY | TYPE_INT8_ARRAY | TYPE_INT16_ARRAY
            | TYPE_INT32_ARRAY | TYPE_INT64_ARRAY | TYPE_UINT16_ARRAY | TYPE_UINT32_ARRAY
            | TYPE_UINT64_ARRAY | TYPE_FLOAT_ARRAY | TYPE_DOUBLE_ARRAY | TYPE_ARRAY => {
                libffi_args.push(Arg::new(&ptr_vals[ptr_idx]));
                ptr_idx += 1;
            }
            _ => {
                return Err(FfiError::new(
                    ERROR_INVALID_SIGNATURE,
                    "unhandled argument type",
                ));
            }
        }
    }

    let code_ptr = CodePtr::from_ptr(sym_ptr as *mut c_void);

    // SAFETY: Foreign C function call mediated by libffi with verified parameter types and lifetimes.
    let updated_payload = unsafe {
        match result_rec.type_code {
            TYPE_VOID => {
                let _: () = cif.call(code_ptr, &libffi_args);
                CValuePayload::Void
            }
            TYPE_INT8 => {
                let val: i8 = cif.call(code_ptr, &libffi_args);
                CValuePayload::Int8(val)
            }
            TYPE_UINT8 => {
                let val: u8 = cif.call(code_ptr, &libffi_args);
                CValuePayload::UInt8(val)
            }
            TYPE_INT16 => {
                let val: i16 = cif.call(code_ptr, &libffi_args);
                CValuePayload::Int16(val)
            }
            TYPE_UINT16 => {
                let val: u16 = cif.call(code_ptr, &libffi_args);
                CValuePayload::UInt16(val)
            }
            TYPE_INT32 | TYPE_INT => {
                let val: i32 = cif.call(code_ptr, &libffi_args);
                if result_rec.type_code == TYPE_INT {
                    CValuePayload::Int(val)
                } else {
                    CValuePayload::Int32(val)
                }
            }
            TYPE_UINT32 | TYPE_UINT => {
                let val: u32 = cif.call(code_ptr, &libffi_args);
                if result_rec.type_code == TYPE_UINT {
                    CValuePayload::UInt(val)
                } else {
                    CValuePayload::UInt32(val)
                }
            }
            TYPE_INT64 => {
                let val: i64 = cif.call(code_ptr, &libffi_args);
                CValuePayload::Int64(val)
            }
            TYPE_UINT64 => {
                let val: u64 = cif.call(code_ptr, &libffi_args);
                CValuePayload::UInt64(val)
            }
            TYPE_LONG => {
                let val: libc::c_long = cif.call(code_ptr, &libffi_args);
                CValuePayload::Long(val as i64)
            }
            TYPE_ULONG => {
                let val: libc::c_ulong = cif.call(code_ptr, &libffi_args);
                CValuePayload::ULong(val as u64)
            }
            TYPE_SIZE_T => {
                let val: usize = cif.call(code_ptr, &libffi_args);
                CValuePayload::SizeT(val as u64)
            }
            TYPE_FLOAT => {
                let val: f32 = cif.call(code_ptr, &libffi_args);
                CValuePayload::Float(val)
            }
            TYPE_DOUBLE => {
                let val: f64 = cif.call(code_ptr, &libffi_args);
                CValuePayload::Double(val)
            }
            TYPE_BOOL => {
                let val: u8 = cif.call(code_ptr, &libffi_args);
                CValuePayload::Bool(val != 0)
            }
            TYPE_HANDLE_RESULT => {
                let val: *mut c_void = cif.call(code_ptr, &libffi_args);
                if val.is_null() {
                    CValuePayload::HandleResult(None)
                } else {
                    CValuePayload::HandleResult(Some(val))
                }
            }
            _ => {
                return Err(FfiError::new(
                    ERROR_INVALID_SIGNATURE,
                    "unsupported result type",
                ));
            }
        }
    };

    update_record_payload(result_handle, updated_payload);
    Ok(())
}

/// Reads a NUL-terminated UTF-8 string from a C pointer.
#[allow(unsafe_code)]
pub fn read_c_string(ptr: usize) -> Result<String, FfiError> {
    if ptr == 0 {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "cannot read string from null pointer",
        ));
    }
    // SAFETY: Reading NUL-terminated byte sequence from non-null C pointer.
    let cstr = unsafe { CStr::from_ptr(ptr as *const c_char) };
    match cstr.to_str() {
        Ok(s) => Ok(s.to_string()),
        Err(e) => Err(FfiError::with_cause(
            ERROR_INVALID_UTF8,
            "C string is not valid UTF-8",
            e.to_string(),
        )),
    }
}

/// Copies elements from a C pointer into a Basic Next array buffer.
#[allow(unsafe_code)]
pub fn array_read_from(array_handle: u64, source_ptr: *mut c_void) -> Result<(), FfiError> {
    if source_ptr.is_null() {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "cannot read from null handle pointer",
        ));
    }
    let rec = get_record(array_handle)
        .ok_or_else(|| FfiError::new(ERROR_INVALID_ARGUMENT, "invalid array handle"))?;
    match rec.payload {
        CValuePayload::Array {
            data,
            element_size,
            count,
        } => {
            let total_bytes = element_size * count;
            // SAFETY: Source pointer is non-null; destination is caller-allocated region storage.
            unsafe {
                std::ptr::copy_nonoverlapping(source_ptr as *const u8, data, total_bytes);
            }
            Ok(())
        }
        _ => Err(FfiError::new(
            ERROR_INVALID_SIGNATURE,
            "target record is not an array",
        )),
    }
}

/// Copies elements from a Basic Next array buffer into a C pointer.
#[allow(unsafe_code)]
pub fn array_write_to(array_handle: u64, dest_ptr: *mut c_void) -> Result<(), FfiError> {
    if dest_ptr.is_null() {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "cannot write to null handle pointer",
        ));
    }
    let rec = get_record(array_handle)
        .ok_or_else(|| FfiError::new(ERROR_INVALID_ARGUMENT, "invalid array handle"))?;
    match rec.payload {
        CValuePayload::Array {
            data,
            element_size,
            count,
        } => {
            let total_bytes = element_size * count;
            // SAFETY: Destination pointer is non-null; source is caller-allocated region storage.
            unsafe {
                std::ptr::copy_nonoverlapping(data as *const u8, dest_ptr as *mut u8, total_bytes);
            }
            Ok(())
        }
        _ => Err(FfiError::new(
            ERROR_INVALID_SIGNATURE,
            "source record is not an array",
        )),
    }
}

/// Retrieves the raw pointer value of a Handle record.
pub fn get_handle_ptr(handle_record_id: u64) -> Result<usize, FfiError> {
    let rec = get_record(handle_record_id)
        .ok_or_else(|| FfiError::new(ERROR_INVALID_ARGUMENT, "invalid handle record"))?;
    match rec.payload {
        CValuePayload::Handle(ptr) => Ok(ptr as usize),
        CValuePayload::HandleResult(Some(ptr)) => Ok(ptr as usize),
        CValuePayload::HandleResult(None) => Ok(0),
        _ => Err(FfiError::new(
            ERROR_INVALID_SIGNATURE,
            "record is not a handle or handle result",
        )),
    }
}

/// Reads raw bytes from a pointer into a slice.
#[allow(unsafe_code)]
pub fn read_bytes_from_ptr(ptr: usize, buf: &mut [u8]) -> Result<(), FfiError> {
    if ptr == 0 {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "cannot read from null pointer",
        ));
    }
    // SAFETY: Caller validates buffer length and pointer validity.
    unsafe {
        std::ptr::copy_nonoverlapping(ptr as *const u8, buf.as_mut_ptr(), buf.len());
    }
    Ok(())
}

/// Writes raw bytes to a pointer from a slice.
#[allow(unsafe_code)]
pub fn write_bytes_to_ptr(ptr: usize, buf: &[u8]) -> Result<(), FfiError> {
    if ptr == 0 {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "cannot write to null pointer",
        ));
    }
    // SAFETY: Caller validates buffer length and pointer validity.
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), ptr as *mut u8, buf.len());
    }
    Ok(())
}

/// Appends an argument handle to an Args record.
pub fn args_add(args_handle: u64, item_handle: u64) -> bool {
    let mut map = records_map().lock().expect("lock records");
    if let Some(rec) = map.get_mut(&args_handle) {
        if let CValuePayload::Args(ref mut list) = rec.payload {
            list.push(item_handle);
            return true;
        }
    }
    false
}

/// Returns the number of argument handles in an Args record.
pub fn args_count(args_handle: u64) -> usize {
    let map = records_map().lock().expect("lock records");
    if let Some(rec) = map.get(&args_handle) {
        if let CValuePayload::Args(ref list) = rec.payload {
            return list.len();
        }
    }
    0
}

/// Executes a native C call with arguments supplied via an Args record handle.
pub fn call_c_function_with_args(
    library_name: &str,
    symbol_name: &str,
    ffi_paths: &[PathBuf],
    args_handle: u64,
    result_handle: u64,
) -> Result<(), FfiError> {
    let arg_handles = {
        let map = records_map().lock().expect("lock records");
        let rec = map
            .get(&args_handle)
            .ok_or_else(|| FfiError::new(ERROR_INVALID_ARGUMENT, "invalid args handle"))?;
        match &rec.payload {
            CValuePayload::Args(list) => list.clone(),
            _ => {
                return Err(FfiError::new(
                    ERROR_INVALID_SIGNATURE,
                    "record is not an Args object",
                ));
            }
        }
    };
    call_c_function(
        library_name,
        symbol_name,
        ffi_paths,
        &arg_handles,
        result_handle,
    )
}
