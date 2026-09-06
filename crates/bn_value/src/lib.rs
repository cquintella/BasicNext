//! Runtime value representation shared by the interpreter and host adapters.

use std::collections::HashMap;

use bn_types::{FloatType, IntegerType};

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
    String(String),
    Vector(Vec<Value>),
    Function(String),
    Type(String),
    Null,
    NotAvailable,
    EndOfFile,
    Error {
        code: i32,
        message: String,
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
    DispatchQueue(u64),
    DispatchTicket(u64),
    DispatchGroup(u64),
    DispatchBarrier(u64),
    DispatchSemaphore(u64),
    DispatchMutex(u64),
    File(u64),
    DataFrame(u64),
    Handle {
        type_name: String,
    },
    Record {
        type_name: String,
        fields: HashMap<String, Value>,
    },
    Object {
        handle: Handle,
        class: String,
    },
    Pointer {
        handle: Handle,
    },
    Date(i32),
    Time(u32),
    TimeZone(String),
}
