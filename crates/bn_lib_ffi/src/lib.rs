// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interpreter adapter for `BNFfi`.
//!
//! Exposes foreign C function calling capabilities through standard module `BNFfi`.
//! Zero `unsafe` code lives here; all dynamic loader and libffi interaction
//! is delegated to `bn_rt::ffi`.

#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::match_same_arms,
    clippy::similar_names,
    clippy::manual_let_else,
    clippy::needless_pass_by_value,
    clippy::collapsible_if
)]

use std::collections::HashMap;
use std::path::PathBuf;

use bn_diag::Diagnostic;
use bn_interp::name_not_found;
use bn_interp::provider::{CoreContext, Provider};
use bn_interp::require_arity_pub as require_arity;
use bn_interp::type_mismatch;
use bn_rt::ffi::{
    CValuePayload, ERROR_INVALID_ARGUMENT, ERROR_INVALID_SIGNATURE, ERROR_VALUE_OUT_OF_RANGE,
    TYPE_ARRAY, TYPE_BOOL, TYPE_DOUBLE, TYPE_FLOAT, TYPE_HANDLE, TYPE_HANDLE_RESULT, TYPE_INT,
    TYPE_INT8, TYPE_INT16, TYPE_INT32, TYPE_INT64, TYPE_LONG, TYPE_SIZE_T, TYPE_STRING, TYPE_UINT,
    TYPE_UINT8, TYPE_UINT16, TYPE_UINT32, TYPE_UINT64, TYPE_ULONG, TYPE_VOID, allocate_record,
    call_c_function, get_handle_ptr, get_record, read_bytes_from_ptr, read_c_string,
    release_record, write_bytes_to_ptr,
};
use bn_runtime::Handle;
use bn_source::Span;
use bn_types::{FloatType, IntegerType};
use bn_value::Value;

pub const NAME: &str = "BNFfi";

struct TempArrayGuard {
    records: Vec<(u64, Handle, u32, usize, Vec<u8>)>,
}

impl Drop for TempArrayGuard {
    fn drop(&mut self) {
        for &(temp_id, _, _, _, _) in &self.records {
            release_record(temp_id);
        }
    }
}

#[derive(Debug)]
pub struct FfiProvider {
    ffi_paths: Vec<PathBuf>,
    records: HashMap<Handle, u64>,
    args: HashMap<Handle, Vec<Handle>>,
    arrays: HashMap<Handle, (Handle, u32, usize)>,
}

impl FfiProvider {
    #[must_use]
    pub fn new(ffi_paths: Vec<PathBuf>) -> Self {
        Self {
            ffi_paths,
            records: HashMap::new(),
            args: HashMap::new(),
            arrays: HashMap::new(),
        }
    }

    fn scalar_value(&self, handle: Handle, span: Span) -> Result<Value, Diagnostic> {
        let Some(&rec_id) = self.records.get(&handle) else {
            return Ok(Value::Null);
        };
        let Some(rec) = get_record(rec_id) else {
            return Ok(Value::Null);
        };
        match rec.payload {
            CValuePayload::Int8(v) => Ok(Value::Integer(i128::from(v), IntegerType::Int8)),
            CValuePayload::UInt8(v) => Ok(Value::Integer(i128::from(v), IntegerType::Byte)),
            CValuePayload::Int16(v) => Ok(Value::Integer(i128::from(v), IntegerType::Int16)),
            CValuePayload::UInt16(v) => Ok(Value::Integer(i128::from(v), IntegerType::UInt16)),
            CValuePayload::Int32(v) => Ok(Value::Integer(i128::from(v), IntegerType::Int32)),
            CValuePayload::UInt32(v) => Ok(Value::Integer(i128::from(v), IntegerType::UInt32)),
            CValuePayload::Int64(v) => Ok(Value::Integer(i128::from(v), IntegerType::Int64)),
            CValuePayload::UInt64(v) => Ok(Value::Integer(i128::from(v), IntegerType::UInt64)),
            CValuePayload::Int(v) => Ok(Value::Integer(i128::from(v), IntegerType::Int32)),
            CValuePayload::UInt(v) => Ok(Value::Integer(i128::from(v), IntegerType::UInt32)),
            CValuePayload::Long(v) => Ok(Value::Integer(i128::from(v), IntegerType::Int64)),
            CValuePayload::ULong(v) => Ok(Value::Integer(i128::from(v), IntegerType::UInt64)),
            CValuePayload::SizeT(v) => Ok(Value::Integer(i128::from(v), IntegerType::UInt64)),
            CValuePayload::Float(v) => Ok(Value::Float(f64::from(v), FloatType::Float32)),
            CValuePayload::Double(v) => Ok(Value::Float(v, FloatType::Float64)),
            CValuePayload::Bool(v) => Ok(Value::Boolean(v)),
            _ => Err(type_mismatch("scalar", "non-scalar", "Value()", span)),
        }
    }
}

impl Provider for FfiProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        // Direct function call: C.Call(lib, sym, args, result)
        if member == "Call" || member.ends_with(".Call") {
            return self.call_invoke(core, &arguments, span);
        }

        // Class member calls
        let (class, action) = match member.split_once('.') {
            Some((c, a)) => (c.rsplit('.').next().unwrap_or(c), a),
            None => return Err(name_not_found(member, "BNFfi member", span)),
        };

        if action.starts_with('$') {
            return Ok(Value::Null);
        }

        match action {
            "CONSTRUCTOR" => self.call_constructor(core, class, &arguments, span),
            "Value" => self.call_value(core, class, &arguments, span),
            "ReadString" => self.call_read_string(&arguments, span),
            "ReadFrom" => self.call_read_from(core, &arguments, span),
            "WriteTo" => self.call_write_to(core, &arguments, span),
            "Add" => self.call_args_add(&arguments, span),
            "Count" => self.call_args_count(&arguments, span),
            _ => Err(name_not_found(member, "BNFfi member", span)),
        }
    }

    fn object_destroyed(&mut self, handle: Handle) {
        if let Some(rec_id) = self.records.remove(&handle) {
            release_record(rec_id);
        }
        self.args.remove(&handle);
        self.arrays.remove(&handle);
    }

    fn close_all(&mut self) {
        for &rec_id in self.records.values() {
            release_record(rec_id);
        }
        self.records.clear();
        self.args.clear();
        self.arrays.clear();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl FfiProvider {
    fn call_constructor(
        &mut self,
        _core: &mut dyn CoreContext,
        class: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let expected_arity = match class {
            "Void" | "HandleResult" | "Args" => 1,
            _ => 2,
        };
        require_arity(
            &format!("{class}.CONSTRUCTOR"),
            arguments,
            expected_arity,
            span,
        )?;
        let Value::Object { handle, .. } = arguments[0] else {
            return Err(type_mismatch("Object", "non-object receiver", class, span));
        };

        match class {
            "Int8" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_INT8, CValuePayload::Int8(v as i8));
                self.records.insert(handle, rec);
            }
            "UInt8" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_UINT8, CValuePayload::UInt8(v as u8));
                self.records.insert(handle, rec);
            }
            "Int16" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_INT16, CValuePayload::Int16(v as i16));
                self.records.insert(handle, rec);
            }
            "UInt16" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_UINT16, CValuePayload::UInt16(v as u16));
                self.records.insert(handle, rec);
            }
            "Int32" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_INT32, CValuePayload::Int32(v as i32));
                self.records.insert(handle, rec);
            }
            "UInt32" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_UINT32, CValuePayload::UInt32(v as u32));
                self.records.insert(handle, rec);
            }
            "Int64" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_INT64, CValuePayload::Int64(v as i64));
                self.records.insert(handle, rec);
            }
            "UInt64" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_UINT64, CValuePayload::UInt64(v as u64));
                self.records.insert(handle, rec);
            }
            "Int" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_INT, CValuePayload::Int(v as i32));
                self.records.insert(handle, rec);
            }
            "UInt" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_UINT, CValuePayload::UInt(v as u32));
                self.records.insert(handle, rec);
            }
            "Long" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_LONG, CValuePayload::Long(v as i64));
                self.records.insert(handle, rec);
            }
            "ULong" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_ULONG, CValuePayload::ULong(v as u64));
                self.records.insert(handle, rec);
            }
            "SizeT" => {
                let v = integer_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_SIZE_T, CValuePayload::SizeT(v as u64));
                self.records.insert(handle, rec);
            }
            "Float" => {
                let f = float_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_FLOAT, CValuePayload::Float(f as f32));
                self.records.insert(handle, rec);
            }
            "Double" => {
                let f = float_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_DOUBLE, CValuePayload::Double(f));
                self.records.insert(handle, rec);
            }
            "Bool" => {
                let b = bool_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_BOOL, CValuePayload::Bool(b));
                self.records.insert(handle, rec);
            }
            "String" => {
                let s = string_arg(arguments, 1, class, span)?;
                let rec = allocate_record(TYPE_STRING, CValuePayload::String(s));
                self.records.insert(handle, rec);
            }
            "Void" => {
                let rec = allocate_record(TYPE_VOID, CValuePayload::Void);
                self.records.insert(handle, rec);
            }
            "HandleResult" => {
                let rec = allocate_record(TYPE_HANDLE_RESULT, CValuePayload::HandleResult(None));
                self.records.insert(handle, rec);
            }
            "Args" => {
                self.args.insert(handle, Vec::new());
            }
            "ByteArray" => self.register_array(arguments, handle, TYPE_UINT8, 1, span)?,
            "Int8Array" => self.register_array(arguments, handle, TYPE_INT8, 1, span)?,
            "Int16Array" => self.register_array(arguments, handle, TYPE_INT16, 2, span)?,
            "Int32Array" => self.register_array(arguments, handle, TYPE_INT32, 4, span)?,
            "Int64Array" => self.register_array(arguments, handle, TYPE_INT64, 8, span)?,
            "UInt16Array" => self.register_array(arguments, handle, TYPE_UINT16, 2, span)?,
            "UInt32Array" => self.register_array(arguments, handle, TYPE_UINT32, 4, span)?,
            "UInt64Array" => self.register_array(arguments, handle, TYPE_UINT64, 8, span)?,
            "FloatArray" => self.register_array(arguments, handle, TYPE_FLOAT, 4, span)?,
            "DoubleArray" => self.register_array(arguments, handle, TYPE_DOUBLE, 8, span)?,
            _ => return Err(name_not_found(class, "BNFfi class constructor", span)),
        }
        Ok(Value::Null)
    }

    fn register_array(
        &mut self,
        arguments: &[Value],
        handle: Handle,
        type_code: u32,
        element_size: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Value::Pointer { handle: region, .. } = arguments[1] else {
            return Err(type_mismatch(
                "Region Pointer",
                "non-pointer",
                "Array constructor",
                span,
            ));
        };
        self.arrays
            .insert(handle, (region, type_code, element_size));
        Ok(())
    }

    fn call_value(
        &mut self,
        core: &mut dyn CoreContext,
        class: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(&format!("{class}.Value"), arguments, 1, span)?;
        let Value::Object { handle, .. } = arguments[0] else {
            return Err(type_mismatch("Object", "non-object receiver", class, span));
        };

        if class == "HandleResult" {
            let Some(&rec_id) = self.records.get(&handle) else {
                return Ok(Value::Null);
            };
            let Some(rec) = get_record(rec_id) else {
                return Ok(Value::Null);
            };
            return match rec.payload {
                CValuePayload::HandleResult(Some(ptr)) => {
                    let obj = core.allocate_object("BNFfi.Handle", span)?;
                    let Value::Object {
                        handle: h_handle, ..
                    } = obj
                    else {
                        unreachable!("allocate_object returns Object");
                    };
                    let h_rec = allocate_record(TYPE_HANDLE, CValuePayload::Handle(ptr));
                    self.records.insert(h_handle, h_rec);
                    Ok(obj)
                }
                _ => Ok(Value::Null),
            };
        }

        self.scalar_value(handle, span)
    }

    fn call_read_string(&self, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        require_arity("Handle.ReadString", arguments, 1, span)?;
        let Value::Object { handle, .. } = arguments[0] else {
            return Err(type_mismatch(
                "Handle",
                "non-object receiver",
                "ReadString",
                span,
            ));
        };
        let Some(&rec_id) = self.records.get(&handle) else {
            return Ok(Value::error_report(
                ERROR_INVALID_ARGUMENT,
                "ReadString",
                "invalid handle record".into(),
                String::new(),
            ));
        };
        let ptr = match get_handle_ptr(rec_id) {
            Ok(p) => p,
            Err(e) => {
                return Ok(Value::error_report(
                    e.code,
                    "ReadString",
                    e.message,
                    e.cause.unwrap_or_default(),
                ));
            }
        };
        match read_c_string(ptr) {
            Ok(text) => Ok(Value::String(text.into())),
            Err(e) => Ok(Value::error_report(
                e.code,
                "ReadString",
                e.message,
                e.cause.unwrap_or_default(),
            )),
        }
    }

    fn call_read_from(
        &mut self,
        core: &mut dyn CoreContext,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("Array.ReadFrom", arguments, 2, span)?;
        let Value::Object { handle: arr_h, .. } = arguments[0] else {
            return Err(type_mismatch(
                "Array",
                "non-object receiver",
                "ReadFrom",
                span,
            ));
        };
        let Value::Object { handle: ptr_h, .. } = arguments[1] else {
            return Err(type_mismatch(
                "Handle",
                "non-handle argument",
                "ReadFrom",
                span,
            ));
        };
        let Some(&arr_info) = self.arrays.get(&arr_h) else {
            return Err(type_mismatch(
                "Array",
                "unregistered array",
                "ReadFrom",
                span,
            ));
        };
        let (region_h, type_code, elem_size) = arr_info;
        let Some(&ptr_rec_id) = self.records.get(&ptr_h) else {
            return Ok(Value::error_report(
                ERROR_INVALID_ARGUMENT,
                "ReadFrom",
                "invalid handle record".into(),
                String::new(),
            ));
        };
        let source_ptr = match get_handle_ptr(ptr_rec_id) {
            Ok(p) => p,
            Err(e) => {
                return Ok(Value::error_report(
                    e.code,
                    "ReadFrom",
                    e.message,
                    e.cause.unwrap_or_default(),
                ));
            }
        };

        let len = core.memory().len(region_h, span)?;
        let Some(total_bytes) = len.checked_mul(elem_size) else {
            return Ok(Value::error_report(
                ERROR_VALUE_OUT_OF_RANGE,
                "ReadFrom",
                "array byte size overflow".into(),
                String::new(),
            ));
        };
        let mut buf = vec![0u8; total_bytes];
        if let Err(e) = read_bytes_from_ptr(source_ptr, &mut buf) {
            return Ok(Value::error_report(
                e.code,
                "ReadFrom",
                e.message,
                e.cause.unwrap_or_default(),
            ));
        }

        // Deserialize bytes into region
        for i in 0..len {
            let offset = i * elem_size;
            let val = decode_element(&buf[offset..offset + elem_size], type_code);
            *core.memory_mut().get_mut(region_h, i, span)? = val;
        }

        Ok(Value::Null)
    }

    fn call_write_to(
        &mut self,
        core: &mut dyn CoreContext,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("Array.WriteTo", arguments, 2, span)?;
        let Value::Object { handle: arr_h, .. } = arguments[0] else {
            return Err(type_mismatch(
                "Array",
                "non-object receiver",
                "WriteTo",
                span,
            ));
        };
        let Value::Object { handle: ptr_h, .. } = arguments[1] else {
            return Err(type_mismatch(
                "Handle",
                "non-handle argument",
                "WriteTo",
                span,
            ));
        };
        let Some(&arr_info) = self.arrays.get(&arr_h) else {
            return Err(type_mismatch(
                "Array",
                "unregistered array",
                "WriteTo",
                span,
            ));
        };
        let (region_h, type_code, elem_size) = arr_info;
        let Some(&ptr_rec_id) = self.records.get(&ptr_h) else {
            return Ok(Value::error_report(
                ERROR_INVALID_ARGUMENT,
                "WriteTo",
                "invalid handle record".into(),
                String::new(),
            ));
        };
        let dest_ptr = match get_handle_ptr(ptr_rec_id) {
            Ok(p) => p,
            Err(e) => {
                return Ok(Value::error_report(
                    e.code,
                    "WriteTo",
                    e.message,
                    e.cause.unwrap_or_default(),
                ));
            }
        };

        let len = core.memory().len(region_h, span)?;
        let Some(total_bytes) = len.checked_mul(elem_size) else {
            return Ok(Value::error_report(
                ERROR_VALUE_OUT_OF_RANGE,
                "WriteTo",
                "array byte size overflow".into(),
                String::new(),
            ));
        };
        let mut buf = vec![0u8; total_bytes];

        // Serialize region values into bytes
        for i in 0..len {
            let val = core.memory().get(region_h, i, span)?;
            let offset = i * elem_size;
            encode_element(val, type_code, &mut buf[offset..offset + elem_size]);
        }

        if let Err(e) = write_bytes_to_ptr(dest_ptr, &buf) {
            return Ok(Value::error_report(
                e.code,
                "WriteTo",
                e.message,
                e.cause.unwrap_or_default(),
            ));
        }

        Ok(Value::Null)
    }

    fn call_args_add(&mut self, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        require_arity("Args.Add", arguments, 2, span)?;
        let Value::Object { handle: args_h, .. } = arguments[0] else {
            return Err(type_mismatch("Args", "non-object receiver", "Add", span));
        };
        let Value::Object { handle: val_h, .. } = arguments[1] else {
            return Err(type_mismatch("Value", "non-object argument", "Add", span));
        };
        self.args.entry(args_h).or_default().push(val_h);
        Ok(Value::Null)
    }

    fn call_args_count(&self, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        require_arity("Args.Count", arguments, 1, span)?;
        let Value::Object { handle: args_h, .. } = arguments[0] else {
            return Err(type_mismatch("Args", "non-object receiver", "Count", span));
        };
        let count = self.args.get(&args_h).map_or(0, Vec::len);
        Ok(Value::Integer(
            i128::try_from(count).unwrap_or(i128::MAX),
            IntegerType::Int32,
        ))
    }

    fn call_invoke(
        &mut self,
        core: &mut dyn CoreContext,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("C.Call", arguments, 4, span)?;
        let Value::String(library) = &arguments[0] else {
            return Err(type_mismatch("STRING", "non-string library", "Call", span));
        };
        let Value::String(symbol) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-string symbol", "Call", span));
        };
        let Value::Object { handle: args_h, .. } = arguments[2] else {
            return Err(type_mismatch("Args", "non-args parameter", "Call", span));
        };
        let Value::Object { handle: res_h, .. } = arguments[3] else {
            return Err(type_mismatch("Value", "non-value result", "Call", span));
        };

        let Some(&res_rec_id) = self.records.get(&res_h) else {
            return Ok(Value::error_report(
                ERROR_INVALID_SIGNATURE,
                "Call",
                "result object has no registered record".into(),
                String::new(),
            ));
        };

        let arg_handles = self.args.get(&args_h).cloned().unwrap_or_default();
        let mut arg_rec_ids = Vec::with_capacity(arg_handles.len());
        let mut temp_guard = TempArrayGuard {
            records: Vec::new(),
        };

        for arg_h in arg_handles {
            if let Some(&rec_id) = self.records.get(&arg_h) {
                arg_rec_ids.push(rec_id);
            } else if let Some(&(region_h, type_code, elem_size)) = self.arrays.get(&arg_h) {
                // Prepare array buffer for passing to C
                let len = core.memory().len(region_h, span)?;
                let Some(total_bytes) = len.checked_mul(elem_size) else {
                    return Ok(Value::error_report(
                        ERROR_VALUE_OUT_OF_RANGE,
                        "Call",
                        "array byte size overflow".into(),
                        String::new(),
                    ));
                };
                let mut buf = vec![0u8; total_bytes];
                for i in 0..len {
                    let val = core.memory().get(region_h, i, span)?;
                    let offset = i * elem_size;
                    encode_element(val, type_code, &mut buf[offset..offset + elem_size]);
                }
                temp_guard
                    .records
                    .push((0, region_h, type_code, elem_size, buf));
                let entry = temp_guard.records.last_mut().expect("just pushed");
                let data_ptr = entry.4.as_mut_ptr();
                let temp_id = allocate_record(
                    TYPE_ARRAY,
                    CValuePayload::Array {
                        data: data_ptr,
                        element_size: elem_size,
                        count: len,
                        region_arc_id: None,
                    },
                );
                entry.0 = temp_id;
                arg_rec_ids.push(temp_id);
            } else {
                return Ok(Value::error_report(
                    ERROR_INVALID_SIGNATURE,
                    "Call",
                    "unrecognized argument record".into(),
                    String::new(),
                ));
            }
        }

        let result = call_c_function(
            library.as_ref(),
            symbol.as_ref(),
            &self.ffi_paths,
            &arg_rec_ids,
            res_rec_id,
        );

        // Copy back array contents if modified by C function
        for &(_, region_h, type_code, elem_size, ref buf) in &temp_guard.records {
            let len = buf.len() / elem_size;
            for i in 0..len {
                let offset = i * elem_size;
                let val = decode_element(&buf[offset..offset + elem_size], type_code);
                *core.memory_mut().get_mut(region_h, i, span)? = val;
            }
        }

        match result {
            Ok(()) => Ok(Value::Null),
            Err(e) => Ok(Value::error_report(
                e.code,
                "Call",
                e.message,
                e.cause.unwrap_or_default(),
            )),
        }
    }
}

fn integer_arg(arguments: &[Value], idx: usize, op: &str, span: Span) -> Result<i128, Diagnostic> {
    if idx < arguments.len() {
        if let Value::Integer(i, _) = arguments[idx] {
            return Ok(i);
        }
    }
    Err(type_mismatch("INTEGER", "non-integer", op, span))
}

fn float_arg(arguments: &[Value], idx: usize, op: &str, span: Span) -> Result<f64, Diagnostic> {
    if idx < arguments.len() {
        if let Value::Float(f, _) = arguments[idx] {
            return Ok(f);
        }
    }
    Err(type_mismatch("FLOAT", "non-float", op, span))
}

fn bool_arg(arguments: &[Value], idx: usize, op: &str, span: Span) -> Result<bool, Diagnostic> {
    if idx < arguments.len() {
        if let Value::Boolean(b) = arguments[idx] {
            return Ok(b);
        }
    }
    Err(type_mismatch("BOOLEAN", "non-boolean", op, span))
}

fn string_arg(arguments: &[Value], idx: usize, op: &str, span: Span) -> Result<String, Diagnostic> {
    if idx < arguments.len() {
        if let Value::String(s) = &arguments[idx] {
            return Ok(s.as_ref().to_string());
        }
    }
    Err(type_mismatch("STRING", "non-string", op, span))
}

fn encode_element(val: &Value, type_code: u32, out: &mut [u8]) {
    match type_code {
        TYPE_UINT8 => {
            if let Value::Integer(i, _) = val {
                out[0] = *i as u8;
            }
        }
        TYPE_INT8 => {
            if let Value::Integer(i, _) = val {
                out[0] = *i as u8;
            }
        }
        TYPE_INT16 => {
            if let Value::Integer(i, _) = val {
                out.copy_from_slice(&(*i as i16).to_ne_bytes());
            }
        }
        TYPE_UINT16 => {
            if let Value::Integer(i, _) = val {
                out.copy_from_slice(&(*i as u16).to_ne_bytes());
            }
        }
        TYPE_INT32 => {
            if let Value::Integer(i, _) = val {
                out.copy_from_slice(&(*i as i32).to_ne_bytes());
            }
        }
        TYPE_UINT32 => {
            if let Value::Integer(i, _) = val {
                out.copy_from_slice(&(*i as u32).to_ne_bytes());
            }
        }
        TYPE_INT64 => {
            if let Value::Integer(i, _) = val {
                out.copy_from_slice(&(*i as i64).to_ne_bytes());
            }
        }
        TYPE_UINT64 => {
            if let Value::Integer(i, _) = val {
                out.copy_from_slice(&(*i as u64).to_ne_bytes());
            }
        }
        TYPE_FLOAT => {
            if let Value::Float(f, _) = val {
                out.copy_from_slice(&(*f as f32).to_ne_bytes());
            }
        }
        TYPE_DOUBLE => {
            if let Value::Float(f, _) = val {
                out.copy_from_slice(&f.to_ne_bytes());
            }
        }
        _ => {}
    }
}

fn decode_element(bytes: &[u8], type_code: u32) -> Value {
    match type_code {
        TYPE_UINT8 => Value::Integer(i128::from(bytes[0]), IntegerType::Byte),
        TYPE_INT8 => Value::Integer(i128::from(bytes[0] as i8), IntegerType::Int8),
        TYPE_INT16 => {
            let mut arr = [0u8; 2];
            arr.copy_from_slice(bytes);
            Value::Integer(i128::from(i16::from_ne_bytes(arr)), IntegerType::Int16)
        }
        TYPE_UINT16 => {
            let mut arr = [0u8; 2];
            arr.copy_from_slice(bytes);
            Value::Integer(i128::from(u16::from_ne_bytes(arr)), IntegerType::UInt16)
        }
        TYPE_INT32 => {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(bytes);
            Value::Integer(i128::from(i32::from_ne_bytes(arr)), IntegerType::Int32)
        }
        TYPE_UINT32 => {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(bytes);
            Value::Integer(i128::from(u32::from_ne_bytes(arr)), IntegerType::UInt32)
        }
        TYPE_INT64 => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Value::Integer(i128::from(i64::from_ne_bytes(arr)), IntegerType::Int64)
        }
        TYPE_UINT64 => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Value::Integer(i128::from(u64::from_ne_bytes(arr)), IntegerType::UInt64)
        }
        TYPE_FLOAT => {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(bytes);
            Value::Float(f64::from(f32::from_ne_bytes(arr)), FloatType::Float32)
        }
        TYPE_DOUBLE => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Value::Float(f64::from_ne_bytes(arr), FloatType::Float64)
        }
        _ => Value::Null,
    }
}
