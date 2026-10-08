//! Runtime value representation shared by the interpreter and host adapters.

use std::sync::Arc;

use bn_types::{FloatType, IntegerType};

pub type SharedString = Arc<str>;

#[must_use]
pub fn shared_string(value: impl Into<SharedString>) -> SharedString {
    value.into()
}

/// Interpreter-private positional storage for a validated BN record/class value.
///
/// Field spellings and IR identities deliberately do not enter this crate: the
/// frontend resolves them to checked slots before execution.
#[derive(Clone, Debug)]
pub struct RecordValue {
    type_name: SharedString,
    fields: Box<[Value]>,
}

impl RecordValue {
    #[must_use]
    pub fn new(type_name: impl Into<SharedString>, fields: Vec<Value>) -> Self {
        Self {
            type_name: type_name.into(),
            fields: fields.into_boxed_slice(),
        }
    }

    #[must_use]
    pub fn type_name(&self) -> &SharedString {
        &self.type_name
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    #[must_use]
    pub fn get(&self, slot: usize) -> Option<&Value> {
        self.fields.get(slot)
    }

    pub fn get_mut(&mut self, slot: usize) -> Option<&mut Value> {
        self.fields.get_mut(slot)
    }

    pub fn replace(&mut self, slot: usize, value: Value) -> Option<Value> {
        self.fields
            .get_mut(slot)
            .map(|field| std::mem::replace(field, value))
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.fields.iter()
    }

    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Value> {
        self.fields.iter_mut()
    }

    #[must_use]
    pub fn into_fields(self) -> Box<[Value]> {
        self.fields
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Handle {
    pub slot: u32,
    pub generation: u32,
}

impl Handle {
    #[must_use]
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }

    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Integer(i128, IntegerType),
    Float(f64, FloatType),
    Boolean(bool),
    String(SharedString),
    Vector(Vec<Value>),
    Function(SharedString),
    Type(SharedString),
    Null,
    NotAvailable,
    EndOfFile,
    /// A BN `Error` (language/0.6/error.md). Build with [`Value::error`].
    /// `Operation` and `Cause` sit behind one pointer to keep `Value` within
    /// its size budget.
    Error {
        code: i32,
        message: SharedString,
        detail: Option<Arc<ErrorDetail>>,
    },
    HostConsole,
    HostArgs,
    TcpStream(u64),
    TcpListener(u64),
    UdpSocket(u64),
    LogFields(u64),
    LogEntry(u64),
    LogLogger(u64),
    Json(u64),
    CryptoBytes(u64),
    DispatchQueue(u64),
    DispatchTicket(u64),
    DispatchGroup(u64),
    DispatchBarrier(u64),
    DispatchSemaphore(u64),
    DispatchMutex(u64),
    File(u64),
    DataFrame(u64),
    Handle {
        type_name: SharedString,
    },
    Record {
        record: RecordValue,
    },
    Object {
        handle: Handle,
        class: SharedString,
    },
    Pointer {
        handle: Handle,
        element: u32,
    },
    Date(i32),
    Time(u32),
    TimeZone(SharedString),
}

/// The `Operation` and `Cause` of an `Error`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorDetail {
    pub operation: SharedString,
    pub cause: SharedString,
}

impl Value {
    /// An `Error` with `code` and `message`. `Operation` and `Cause` stay
    /// empty until the producer's domain supplies them (bucket 0.6.2b R4-R8).
    #[must_use]
    pub const fn error(code: i32, message: SharedString) -> Self {
        Self::Error {
            code,
            message,
            detail: None,
        }
    }
}

impl Value {
    /// An `Error` with all four fields (language/0.6/error.md).
    #[must_use]
    pub fn error_report(code: i32, operation: &str, message: String, cause: String) -> Self {
        Self::Error {
            code,
            message: SharedString::from(message),
            detail: Some(Arc::new(ErrorDetail {
                operation: SharedString::from(operation),
                cause: SharedString::from(cause),
            })),
        }
    }
}

/// `Error.Operation` and `Error.Cause` of an error's detail (empty without).
#[must_use]
pub fn error_detail_field(detail: Option<&ErrorDetail>, cause: bool) -> SharedString {
    detail.map_or_else(
        || SharedString::from(""),
        |detail| {
            if cause {
                detail.cause.clone()
            } else {
                detail.operation.clone()
            }
        },
    )
}
