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
        Mutex, OnceLock, PoisonError,
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
pub const MSG_ARRAY_BYTE_SIZE_OVERFLOW: &str = "array byte size overflow";

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
        region_arc_id: Option<u64>,
    },
}

// SAFETY: Pointers in CValuePayload represent memory buffers whose lifetimes are
// managed by the Basic Next caller or the foreign library.
#[allow(unsafe_code)]
unsafe impl Send for CValuePayload {}
#[allow(unsafe_code)]
unsafe impl Sync for CValuePayload {}

#[derive(Debug, Clone)]
pub struct Record {
    pub type_code: u32,
    pub payload: CValuePayload,
    pub child_handle: Option<u64>,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static RECORDS: OnceLock<Mutex<HashMap<u64, Record>>> = OnceLock::new();

fn records_map() -> &'static Mutex<HashMap<u64, Record>> {
    RECORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn allocate_record(type_code: u32, payload: CValuePayload) -> u64 {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    let mut map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
    map.insert(
        handle,
        Record {
            type_code,
            payload,
            child_handle: None,
        },
    );
    handle
}

pub fn release_record(handle: u64) {
    if handle == 0 {
        return;
    }
    let mut map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(removed) = map.remove(&handle) {
        if let Some(child_id) = removed.child_handle {
            map.remove(&child_id);
        }
    }
}

pub fn get_record(handle: u64) -> Option<Record> {
    let map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
    map.get(&handle).cloned()
}

pub fn active_record_count() -> usize {
    records_map()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len()
}

pub fn update_record_payload(handle: u64, payload: CValuePayload) -> bool {
    let mut map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
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

/// Returns or creates the cached `TYPE_HANDLE` record associated with a
/// `HandleResult` record when its pointer is non-null. Reusing the child
/// record prevents memory leaks when `hr.Value()` is called repeatedly or when
/// the returned `C.Handle OR NULL` leaves scope (`ISS-04`).
pub fn handle_result_child_handle(handle: u64) -> Option<u64> {
    let mut map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
    let (ptr, existing_child) = match map.get(&handle) {
        Some(Record {
            payload: CValuePayload::HandleResult(Some(ptr)),
            child_handle,
            ..
        }) => (*ptr, *child_handle),
        _ => return None,
    };

    if let Some(child_id) = existing_child {
        if let Some(child_rec) = map.get_mut(&child_id) {
            child_rec.payload = CValuePayload::Handle(ptr);
            return Some(child_id);
        }
    }

    let child_id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    if let Some(parent) = map.get_mut(&handle) {
        parent.child_handle = Some(child_id);
    }
    map.insert(
        child_id,
        Record {
            type_code: TYPE_HANDLE,
            payload: CValuePayload::Handle(ptr),
            child_handle: None,
        },
    );
    Some(child_id)
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

/// Stable per-argument storage kept alive for the duration of `cif.call`.
enum PreparedArg {
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Long(libc::c_long),
    ULong(libc::c_ulong),
    SizeT(usize),
    Float(f32),
    Double(f64),
    Bool(u8),
    Ptr(*mut c_void),
    CString { _owned: CString, ptr: *const c_char },
}

impl PreparedArg {
    fn as_libffi_arg(&self) -> Arg<'_> {
        match self {
            Self::Int8(v) => Arg::new(v),
            Self::UInt8(v) => Arg::new(v),
            Self::Int16(v) => Arg::new(v),
            Self::UInt16(v) => Arg::new(v),
            Self::Int32(v) => Arg::new(v),
            Self::UInt32(v) => Arg::new(v),
            Self::Int64(v) => Arg::new(v),
            Self::UInt64(v) => Arg::new(v),
            Self::Long(v) => Arg::new(v),
            Self::ULong(v) => Arg::new(v),
            Self::SizeT(v) => Arg::new(v),
            Self::Float(v) => Arg::new(v),
            Self::Double(v) => Arg::new(v),
            Self::Bool(v) => Arg::new(v),
            Self::Ptr(v) => Arg::new(v),
            Self::CString { ptr, .. } => Arg::new(ptr),
        }
    }
}

fn prepare_arg(rec: &Record) -> Result<PreparedArg, FfiError> {
    match rec.type_code {
        TYPE_INT8 => match rec.payload {
            CValuePayload::Int8(v) => Ok(PreparedArg::Int8(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Int8 payload",
            )),
        },
        TYPE_UINT8 => match rec.payload {
            CValuePayload::UInt8(v) => Ok(PreparedArg::UInt8(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected UInt8 payload",
            )),
        },
        TYPE_INT16 => match rec.payload {
            CValuePayload::Int16(v) => Ok(PreparedArg::Int16(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Int16 payload",
            )),
        },
        TYPE_UINT16 => match rec.payload {
            CValuePayload::UInt16(v) => Ok(PreparedArg::UInt16(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected UInt16 payload",
            )),
        },
        TYPE_INT32 | TYPE_INT => match rec.payload {
            CValuePayload::Int32(v) | CValuePayload::Int(v) => Ok(PreparedArg::Int32(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Int32/Int payload",
            )),
        },
        TYPE_UINT32 | TYPE_UINT => match rec.payload {
            CValuePayload::UInt32(v) | CValuePayload::UInt(v) => Ok(PreparedArg::UInt32(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected UInt32/UInt payload",
            )),
        },
        TYPE_INT64 => match rec.payload {
            CValuePayload::Int64(v) => Ok(PreparedArg::Int64(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Int64 payload",
            )),
        },
        TYPE_UINT64 => match rec.payload {
            CValuePayload::UInt64(v) => Ok(PreparedArg::UInt64(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected UInt64 payload",
            )),
        },
        TYPE_LONG => match rec.payload {
            CValuePayload::Long(v) | CValuePayload::Int64(v) => {
                let c_val = libc::c_long::try_from(v).map_err(|_| {
                    FfiError::new(
                        ERROR_VALUE_OUT_OF_RANGE,
                        format!("Long value {v} out of range for target C long"),
                    )
                })?;
                Ok(PreparedArg::Long(c_val))
            }
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Long payload",
            )),
        },
        TYPE_ULONG => match rec.payload {
            CValuePayload::ULong(v) | CValuePayload::UInt64(v) => {
                let c_val = libc::c_ulong::try_from(v).map_err(|_| {
                    FfiError::new(
                        ERROR_VALUE_OUT_OF_RANGE,
                        format!("ULong value {v} out of range for target C unsigned long"),
                    )
                })?;
                Ok(PreparedArg::ULong(c_val))
            }
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected ULong payload",
            )),
        },
        _ => prepare_non_integer_arg(rec),
    }
}

fn prepare_non_integer_arg(rec: &Record) -> Result<PreparedArg, FfiError> {
    match rec.type_code {
        TYPE_SIZE_T => match rec.payload {
            CValuePayload::SizeT(v) | CValuePayload::UInt64(v) => {
                let c_val = usize::try_from(v).map_err(|_| {
                    FfiError::new(
                        ERROR_VALUE_OUT_OF_RANGE,
                        format!("SizeT value {v} out of range for target size_t"),
                    )
                })?;
                Ok(PreparedArg::SizeT(c_val))
            }
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected SizeT payload",
            )),
        },
        TYPE_FLOAT => match rec.payload {
            CValuePayload::Float(v) => Ok(PreparedArg::Float(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Float payload",
            )),
        },
        TYPE_DOUBLE => match rec.payload {
            CValuePayload::Double(v) => Ok(PreparedArg::Double(v)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Double payload",
            )),
        },
        TYPE_BOOL => match rec.payload {
            CValuePayload::Bool(v) => Ok(PreparedArg::Bool(if v { 1u8 } else { 0u8 })),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Bool payload",
            )),
        },
        TYPE_STRING => match &rec.payload {
            CValuePayload::String(s) => {
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
                Ok(PreparedArg::CString { _owned: cs, ptr })
            }
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected String payload",
            )),
        },
        TYPE_HANDLE => match rec.payload {
            CValuePayload::Handle(p) => Ok(PreparedArg::Ptr(p)),
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Handle payload",
            )),
        },
        TYPE_BYTE_ARRAY | TYPE_INT8_ARRAY | TYPE_INT16_ARRAY | TYPE_INT32_ARRAY
        | TYPE_INT64_ARRAY | TYPE_UINT16_ARRAY | TYPE_UINT32_ARRAY | TYPE_UINT64_ARRAY
        | TYPE_FLOAT_ARRAY | TYPE_DOUBLE_ARRAY | TYPE_ARRAY => match rec.payload {
            CValuePayload::Array {
                data,
                region_arc_id,
                ..
            } => {
                if let Some(arc_id) = region_arc_id
                    && !crate::arc_abi::is_object_alive(arc_id)
                {
                    crate::arc_abi::abort_use_after_release();
                }
                Ok(PreparedArg::Ptr(data as *mut c_void))
            }
            _ => Err(FfiError::new(
                ERROR_INVALID_SIGNATURE,
                "expected Array payload",
            )),
        },
        _ => Err(FfiError::new(
            ERROR_INVALID_SIGNATURE,
            "unsupported argument type",
        )),
    }
}

#[allow(unsafe_code)]
fn invoke_cif(
    cif: &Cif,
    code_ptr: CodePtr,
    libffi_args: &[Arg<'_>],
    result_type_code: u32,
) -> Result<CValuePayload, FfiError> {
    // SAFETY: Foreign C function call mediated by libffi with verified parameter types and lifetimes.
    let updated_payload = unsafe {
        match result_type_code {
            TYPE_VOID => {
                let _: () = cif.call(code_ptr, libffi_args);
                CValuePayload::Void
            }
            TYPE_INT8 => CValuePayload::Int8(cif.call(code_ptr, libffi_args)),
            TYPE_UINT8 => CValuePayload::UInt8(cif.call(code_ptr, libffi_args)),
            TYPE_INT16 => CValuePayload::Int16(cif.call(code_ptr, libffi_args)),
            TYPE_UINT16 => CValuePayload::UInt16(cif.call(code_ptr, libffi_args)),
            TYPE_INT32 | TYPE_INT => {
                let val: i32 = cif.call(code_ptr, libffi_args);
                if result_type_code == TYPE_INT {
                    CValuePayload::Int(val)
                } else {
                    CValuePayload::Int32(val)
                }
            }
            TYPE_UINT32 | TYPE_UINT => {
                let val: u32 = cif.call(code_ptr, libffi_args);
                if result_type_code == TYPE_UINT {
                    CValuePayload::UInt(val)
                } else {
                    CValuePayload::UInt32(val)
                }
            }
            TYPE_INT64 => CValuePayload::Int64(cif.call(code_ptr, libffi_args)),
            TYPE_UINT64 => CValuePayload::UInt64(cif.call(code_ptr, libffi_args)),
            TYPE_LONG => {
                let val: libc::c_long = cif.call(code_ptr, libffi_args);
                CValuePayload::Long(val as i64)
            }
            TYPE_ULONG => {
                let val: libc::c_ulong = cif.call(code_ptr, libffi_args);
                CValuePayload::ULong(val as u64)
            }
            TYPE_SIZE_T => {
                let val: usize = cif.call(code_ptr, libffi_args);
                CValuePayload::SizeT(val as u64)
            }
            TYPE_FLOAT => CValuePayload::Float(cif.call(code_ptr, libffi_args)),
            TYPE_DOUBLE => CValuePayload::Double(cif.call(code_ptr, libffi_args)),
            TYPE_BOOL => {
                let val: u8 = cif.call(code_ptr, libffi_args);
                CValuePayload::Bool(val != 0)
            }
            TYPE_HANDLE_RESULT => {
                let val: *mut c_void = cif.call(code_ptr, libffi_args);
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
    Ok(updated_payload)
}

/// Executes a native C call using libffi.
///
/// Crucial invariant: No mutex lock is held while executing the foreign call.
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

    let prepared_args: Vec<PreparedArg> = arg_records
        .iter()
        .map(prepare_arg)
        .collect::<Result<Vec<_>, _>>()?;
    let libffi_args: Vec<Arg<'_>> = prepared_args
        .iter()
        .map(PreparedArg::as_libffi_arg)
        .collect();

    let code_ptr = CodePtr::from_ptr(sym_ptr as *mut c_void);
    let updated_payload = invoke_cif(&cif, code_ptr, &libffi_args, result_rec.type_code)?;

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
            region_arc_id,
        } => {
            if let Some(arc_id) = region_arc_id
                && !crate::arc_abi::is_object_alive(arc_id)
            {
                crate::arc_abi::abort_use_after_release();
            }
            if data.is_null() {
                return Err(FfiError::new(
                    ERROR_INVALID_ARGUMENT,
                    "array buffer pointer is null",
                ));
            }
            let total_bytes = element_size.checked_mul(count).ok_or_else(|| {
                FfiError::new(ERROR_VALUE_OUT_OF_RANGE, MSG_ARRAY_BYTE_SIZE_OVERFLOW)
            })?;
            // SAFETY: Source and destination pointers are non-null, region is verified live,
            // and byte size is checked against arithmetic overflow.
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
            region_arc_id,
        } => {
            if let Some(arc_id) = region_arc_id
                && !crate::arc_abi::is_object_alive(arc_id)
            {
                crate::arc_abi::abort_use_after_release();
            }
            if data.is_null() {
                return Err(FfiError::new(
                    ERROR_INVALID_ARGUMENT,
                    "array buffer pointer is null",
                ));
            }
            let total_bytes = element_size.checked_mul(count).ok_or_else(|| {
                FfiError::new(ERROR_VALUE_OUT_OF_RANGE, MSG_ARRAY_BYTE_SIZE_OVERFLOW)
            })?;
            // SAFETY: Destination and source pointers are non-null, region is verified live,
            // and byte size is checked against arithmetic overflow.
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
    let mut map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
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
    let map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
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
        let map = records_map().lock().unwrap_or_else(PoisonError::into_inner);
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
