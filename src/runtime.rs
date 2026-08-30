// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    io::IsTerminal,
    io::{BufRead, Read, Write},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    dataframe::{
        DataFrameColumn, DataFrameJoin, DataFrameResource, duplicate_column_names, join_dataframes,
        parse_csv,
    },
    diagnostic::Diagnostic,
    heap::{Handle, Heap},
    ir::{BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator, ValueId},
    module_graph::ModuleId,
    semantic::{FloatType, IntegerType, PointerLength, SymbolId, Type, integer_byte_size},
    source::Span,
    temporal,
};

#[derive(Clone, Debug)]
pub(crate) enum Value {
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
    #[allow(dead_code)]
    Error {
        code: i32,
        message: String,
    },
    HostConsole,
    HostArgs,
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

/// Host-supplied arguments and clocks for one `bn run` execution.
pub struct HostEnv {
    arguments: Vec<String>,
    clock: ClockKind,
    random_state: Cell<u64>,
    filesystem: bool,
}

enum ClockKind {
    System,
    Fixed {
        timestamp_ms: i64,
        monotonic_ns: i64,
    },
}

impl HostEnv {
    #[must_use]
    pub fn system(arguments: Vec<String>) -> Self {
        Self {
            arguments,
            clock: ClockKind::System,
            random_state: Cell::new(host_random_seed()),
            filesystem: true,
        }
    }

    #[must_use]
    pub fn fixed(arguments: Vec<String>, timestamp_ms: i64, monotonic_ns: i64) -> Self {
        Self {
            arguments,
            clock: ClockKind::Fixed {
                timestamp_ms,
                monotonic_ns,
            },
            random_state: Cell::new(1),
            filesystem: true,
        }
    }

    /// Creates an environment that denies filesystem capability imports.
    #[must_use]
    pub fn without_filesystem(mut self) -> Self {
        self.filesystem = false;
        self
    }

    fn timestamp_ms(&self) -> i64 {
        match self.clock {
            ClockKind::Fixed { timestamp_ms, .. } => timestamp_ms,
            ClockKind::System => system_timestamp_ms(SystemTime::now()),
        }
    }

    fn monotonic_ns(&self) -> i64 {
        match self.clock {
            ClockKind::Fixed { monotonic_ns, .. } => monotonic_ns,
            ClockKind::System => {
                i64::try_from(process_origin().elapsed().as_nanos()).unwrap_or(i64::MAX)
            }
        }
    }
}

fn system_timestamp_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        Err(error) => i64::try_from(error.duration().as_millis()).map_or(i64::MIN, |value| -value),
    }
}

fn host_random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let seed = u64::try_from(nanos).unwrap_or(u64::MAX) ^ u64::from(std::process::id());
    seed.max(1)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{default_span, host_random_seed, integer_from_i128_count, system_timestamp_ms};

    #[test]
    fn system_random_seed_is_never_zero() {
        assert_ne!(host_random_seed(), 0);
    }

    #[test]
    fn system_timestamp_before_epoch_is_negative() {
        assert_eq!(
            system_timestamp_ms(UNIX_EPOCH - Duration::from_millis(1)),
            -1
        );
    }

    #[test]
    fn integer_count_rejects_values_above_the_language_limit() {
        let error = integer_from_i128_count(i128::from(i32::MAX) + 1, default_span())
            .expect_err("INTEGER count overflow");
        assert_eq!(error.code, "NUMERIC_OVERFLOW");
    }
}

fn process_origin() -> Instant {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *ORIGIN.get_or_init(Instant::now)
}

#[derive(Clone)]
struct Instance {
    fields: HashMap<String, Value>,
}

struct Executor<'a> {
    module: &'a Module,
    input: &'a mut dyn BufRead,
    output: &'a mut dyn Write,
    host: &'a HostEnv,
    stop_code: Option<i128>,
    statics: HashMap<(String, String), Value>,
    class_init: HashMap<String, ClassInit>,
    objects: Heap<Instance>,
    memory: Heap<Value>,
    pinned_dispatch: Vec<(Handle, String)>,
    files: HashMap<u64, FileResource>,
    next_file: u64,
    dataframes: HashMap<u64, DataFrameResource>,
    next_dataframe: u64,
}

struct FileResource {
    file: Option<std::fs::File>,
    family: Option<bool>, // ponytail: one bit for text/binary; expand only if modes grow.
}

#[derive(Clone, Copy)]
enum ClassInit {
    Running,
    Ready,
}

/// Executes the `Start` function of a validated BN IR module.
///
/// # Errors
///
/// Returns a source-spanned runtime diagnostic for invalid operations, missing
/// entry points, overflow, invalid indices, or I/O failures.
pub fn execute(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<u8, Diagnostic> {
    let host = HostEnv::system(vec!["bn".into()]);
    execute_with_host(module, input, output, &host)
}

/// Executes `Start` with injected command-line arguments and clocks.
///
/// # Errors
///
/// Returns a source-spanned runtime diagnostic for invalid operations, missing
/// entry points, overflow, invalid indices, or I/O failures.
pub fn execute_with_host(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
) -> Result<u8, Diagnostic> {
    if !host.filesystem
        && let Some(span) = module.filesystem_import
    {
        return Err(runtime_error(
            "HOST_CAPABILITY_UNAVAILABLE",
            "HOST.FileSystem is not provided by this host",
            span,
        ));
    }
    let start = module
        .functions
        .iter()
        .find(|function| function.name == "Start")
        .ok_or_else(|| {
            runtime_error(
                "START_NOT_FOUND",
                "executable module requires FUNCTION Start",
                default_span(),
            )
        })?;
    if !start.parameters.is_empty() {
        return Err(runtime_error(
            "INVALID_START",
            "FUNCTION Start cannot declare parameters",
            start.span,
        ));
    }
    let mut executor = Executor {
        module,
        input,
        output,
        host,
        stop_code: None,
        statics: HashMap::new(),
        class_init: HashMap::new(),
        objects: Heap::default(),
        memory: Heap::default(),
        pinned_dispatch: Vec::new(),
        files: HashMap::new(),
        next_file: 1,
        dataframes: HashMap::new(),
        next_dataframe: 1,
    };
    match executor.function(start, Vec::new())? {
        Flow::Return(None) => Ok(0),
        Flow::Return(Some(Value::Integer(code, _))) | Flow::Stop(code) => {
            exit_code(code, start.span)
        }
        Flow::Return(Some(_)) => Err(runtime_error(
            "INVALID_START",
            "FUNCTION Start must return VOID or INTEGER",
            start.span,
        )),
    }
}

enum Flow {
    Return(Option<Value>),
    Stop(i128),
}

impl Executor<'_> {
    fn function(&mut self, function: &Function, arguments: Vec<Value>) -> Result<Flow, Diagnostic> {
        if arguments.len() != function.parameters.len() {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                format!(
                    "FUNCTION {} expects {} argument(s), found {}",
                    function.name,
                    function.parameters.len(),
                    arguments.len()
                ),
                function.span,
            ));
        }
        let mut symbols = function
            .parameters
            .iter()
            .copied()
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        let mut values = HashMap::new();
        let mut block = function.entry;
        loop {
            let current = find_block(function, block)?;
            for instruction in &current.instructions {
                self.instruction(instruction, &mut symbols, &mut values)?;
                if let Some(code) = self.stop_code.take() {
                    return Ok(Flow::Stop(code));
                }
            }
            match &current.terminator {
                Terminator::Jump { target } => block = *target,
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    block = if boolean(value(&values, *condition, function.span)?, function.span)? {
                        *then_block
                    } else {
                        *else_block
                    };
                }
                Terminator::Return { value: result } => {
                    return Ok(Flow::Return(
                        result
                            .map(|result| value(&values, result, function.span).cloned())
                            .transpose()?,
                    ));
                }
                Terminator::Stop { code } => {
                    let code = integer(value(&values, *code, function.span)?, function.span)?.0;
                    return Ok(Flow::Stop(code));
                }
            }
        }
    }

    fn ensure_class(&mut self, class: &str, span: Span) -> Result<(), Diagnostic> {
        match self.class_init.get(class).copied() {
            Some(ClassInit::Ready) => return Ok(()),
            Some(ClassInit::Running) => {
                return Err(runtime_error(
                    "STATIC_INITIALIZATION_CYCLE",
                    format!("STATIC initialization of {class} reentered"),
                    span,
                ));
            }
            None => {}
        }
        self.class_init
            .insert(class.to_string(), ClassInit::Running);
        let init_name = format!("{class}.$init");
        if let Some(index) = self
            .module
            .functions
            .iter()
            .position(|function| function.name == init_name)
        {
            let function = &self.module.functions[index];
            match self.function(function, Vec::new())? {
                Flow::Stop(code) => self.stop_code = Some(code),
                Flow::Return(_) => {}
            }
        }
        self.class_init.insert(class.to_string(), ClassInit::Ready);
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Runtime operations mirror the compact IR instruction set.
    fn instruction(
        &mut self,
        instruction: &Instruction,
        symbols: &mut HashMap<SymbolId, Value>,
        values: &mut HashMap<ValueId, Value>,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Constant {
                destination,
                value: constant,
                ty,
                span,
            } => set(values, *destination, constant_value(constant, ty, *span)?),
            Instruction::Default {
                destination,
                ty,
                dimensions,
                span,
            } => set(
                values,
                *destination,
                self.default_value(ty, dimensions, *span)?,
            ),
            Instruction::Load {
                destination,
                symbol,
                span,
                ..
            } => {
                let loaded = symbols.get(symbol).cloned().ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                set(values, *destination, loaded);
            }
            Instruction::Store {
                symbol,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                symbols.insert(*symbol, stored);
            }
            Instruction::Copy {
                destination,
                source,
                ty,
                span,
            } => {
                let copied = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                set(values, *destination, copied);
            }
            Instruction::Unary {
                destination,
                operator,
                operand,
                ty,
                span,
            } => {
                let result = unary(operator, value(values, *operand, *span)?, ty, *span)?;
                set(values, *destination, result);
            }
            Instruction::Binary {
                destination,
                operator,
                left,
                right,
                ty,
                span,
            } => {
                let result = binary(
                    operator,
                    value(values, *left, *span)?,
                    value(values, *right, *span)?,
                    ty,
                    *span,
                )?;
                set(values, *destination, result);
            }
            Instruction::Cast {
                destination,
                value: source,
                ty,
                span,
            } => {
                let result = cast(value(values, *source, *span)?.clone(), ty, *span)?;
                set(values, *destination, result);
            }
            Instruction::Call {
                destination,
                callee,
                arguments,
                ty,
                span,
            } => {
                let Value::Function(name) = value(values, *callee, *span)? else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "value is not callable",
                        *span,
                    ));
                };
                let name = name.clone();
                let arguments = arguments
                    .iter()
                    .map(|argument| value(values, *argument, *span).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                let result = self.call_named(&name, arguments, *span)?;
                set(values, *destination, self.coerce_to(result, ty, *span)?);
            }
            Instruction::Input {
                destination, span, ..
            } => {
                let mut line = String::new();
                let count = self.input.read_line(&mut line).map_err(|error| {
                    runtime_error("INPUT_ERROR", format!("cannot read input: {error}"), *span)
                })?;
                let result = if count == 0 {
                    Value::EndOfFile
                } else {
                    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
                        line.pop();
                    }
                    Value::String(line)
                };
                set(values, *destination, result);
            }
            Instruction::Vector {
                destination,
                values: elements,
                span,
                ..
            } => {
                let vector = elements
                    .iter()
                    .map(|element| value(values, *element, *span).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                set(values, *destination, Value::Vector(vector));
            }
            Instruction::Index {
                destination,
                object,
                index,
                span,
                ..
            } => {
                let index = usize::try_from(integer(value(values, *index, *span)?, *span)?.0)
                    .map_err(|_| {
                        runtime_error("INDEX_OUT_OF_BOUNDS", "index cannot be negative", *span)
                    })?;
                let element = self.index_value(value(values, *object, *span)?, index, *span)?;
                set(values, *destination, element);
            }
            Instruction::Member {
                destination,
                object,
                name,
                span,
                ..
            } => {
                let member = self.member_of(value(values, *object, *span)?, name, *span)?;
                set(values, *destination, member);
            }
            Instruction::SetIndex {
                symbol,
                indices,
                value: source,
                ty,
                span,
            } => {
                let indices = indices
                    .iter()
                    .map(|index| {
                        usize::try_from(integer(value(values, *index, *span)?, *span)?.0).map_err(
                            |_| {
                                runtime_error(
                                    "INDEX_OUT_OF_BOUNDS",
                                    "index cannot be negative",
                                    *span,
                                )
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                self.set_index(target, &indices, source, *span)?;
            }
            Instruction::Length {
                destination,
                vector,
                span,
            } => {
                let length = match value(values, *vector, *span)? {
                    Value::Vector(elements) => integer_from_count(elements.len(), *span)?,
                    Value::String(text) => integer_from_count(text.chars().count(), *span)?,
                    Value::HostArgs => integer_from_count(self.host.arguments.len(), *span)?,
                    Value::Pointer { handle } => {
                        integer_from_count(self.memory.len(*handle, *span)?, *span)?
                    }
                    Value::Null => {
                        return Err(runtime_error(
                            "NULL_POINTER_ACCESS",
                            "cannot read the length of a NULL pointer",
                            *span,
                        ));
                    }
                    Value::Integer(_, _) | Value::Float(_, _) => {
                        Value::Integer(1, IntegerType::Int32)
                    }
                    _ => {
                        return Err(runtime_error("TYPE_MISMATCH", "value has no length", *span));
                    }
                };
                set(values, *destination, length);
            }
            Instruction::SizeOf {
                destination,
                value: source,
                span,
            } => {
                let size = self.size_of_value(value(values, *source, *span)?, *span)?;
                set(values, *destination, size);
            }
            Instruction::Print {
                values: printed,
                span,
            } => {
                for printed in printed {
                    write!(self.output, "{}", render(value(values, *printed, *span)?)).map_err(
                        |error| {
                            runtime_error(
                                "OUTPUT_ERROR",
                                format!("cannot write output: {error}"),
                                *span,
                            )
                        },
                    )?;
                }
                writeln!(self.output).map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::ClearScreen { console, span } => {
                require_console(value(values, *console, *span)?, *span)?;
                write!(self.output, "\x1b[2J\x1b[H").map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::Beep { console, span } => {
                require_console(value(values, *console, *span)?, *span)?;
                write!(self.output, "\x07").map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::Allocate {
                destination,
                type_name,
                arguments,
                ty,
                span,
                ..
            } => {
                let allocated = match ty {
                    Type::Pointer { element, .. } => {
                        self.allocate_region(element, arguments, values, *span)?
                    }
                    _ if is_host_file_type(type_name) => {
                        let id = self.next_file;
                        self.next_file += 1;
                        self.files.insert(
                            id,
                            FileResource {
                                file: None,
                                family: None,
                            },
                        );
                        Value::File(id)
                    }
                    _ if self.is_bndata_provider(type_name)
                        && type_name.rsplit('.').next() == Some("DataFrame") =>
                    {
                        let id = self.next_dataframe;
                        self.next_dataframe += 1;
                        self.dataframes.insert(
                            id,
                            DataFrameResource {
                                columns: Vec::new(),
                            },
                        );
                        Value::DataFrame(id)
                    }
                    _ => self.allocate_object(type_name, *span)?,
                };
                set(values, *destination, allocated);
            }
            Instruction::Delete {
                value: deleted,
                destructor,
                span,
            } => {
                let target = value(values, *deleted, *span)?.clone();
                self.delete_value(target, destructor.as_deref(), *span)?;
            }
            Instruction::SetMember {
                object,
                name,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                self.set_member_value(values, *object, name, stored, *span)?;
            }
            Instruction::SetField {
                symbol,
                path,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                self.set_field_path(target, path, stored, *span)?;
            }
            Instruction::EnsureClass { class, span } => self.ensure_class(class, *span)?,
            Instruction::LoadStatic {
                destination,
                class,
                field,
                span,
                ..
            } => {
                let loaded = self.statics.get(&(class.clone(), field.clone())).cloned();
                let loaded = loaded.ok_or_else(|| {
                    runtime_error(
                        "UNINITIALIZED_VALUE",
                        format!("STATIC {class}.{field} has no value"),
                        *span,
                    )
                })?;
                set(values, *destination, loaded);
            }
            Instruction::StoreStatic {
                class,
                field,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                self.statics.insert((class.clone(), field.clone()), stored);
            }
        }
        Ok(())
    }

    fn call_named(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if name.starts_with("HOST.") {
            return self.host_call(name, &arguments, span);
        }
        if is_host_file_method(name) {
            return self.file_call(name, &arguments, span);
        }
        if self.is_bndata_provider(name)
            && (name.ends_with(".CONSTRUCTOR") || name.ends_with(".$fields"))
        {
            return Ok(Value::Null);
        }
        if self.is_bndata_provider(name) && name.contains(".DataFrame.") {
            return self.dataframe_call(name, &arguments, span);
        }
        if self.is_bndata_provider(name)
            && (name.ends_with(".ReadCSV") || name.ends_with(".WriteCSV"))
        {
            return self.data_call(name, &arguments, span);
        }
        if self.is_bnmath_provider(name) {
            let math_name = name.rsplit('.').next().unwrap_or(name);
            let builtin_name = format!("BNMath.{math_name}");
            if is_temporal_builtin(&builtin_name) {
                return temporal_call(&builtin_name, &arguments, span);
            }
            return builtin(&builtin_name, &arguments, span, &self.memory);
        }
        if is_temporal_builtin(name) {
            return temporal_call(name, &arguments, span);
        }
        if name.starts_with("BNMath.") || matches!(name, "ASC" | "CHAR") || name == "$for_condition"
        {
            return builtin(name, &arguments, span, &self.memory);
        }
        let (name, super_call) = name
            .strip_prefix("@super:")
            .map_or((name, false), |name| (name, true));
        let resolved = if super_call {
            name.to_string()
        } else {
            self.dispatch_name(name, &arguments)
        };
        let index = self
            .module
            .functions
            .iter()
            .position(|function| function.name == resolved)
            .ok_or_else(|| {
                runtime_error(
                    "NAME_NOT_FOUND",
                    format!("function '{resolved}' is not available"),
                    span,
                )
            })?;
        let constructed = (name.ends_with(".CONSTRUCTOR") || name.ends_with(".$fields"))
            .then(|| match arguments.first() {
                Some(Value::Object { handle, .. }) => Some(*handle),
                _ => None,
            })
            .flatten();
        let pinned = lifecycle_dispatch(&resolved, &arguments);
        if let Some(pinned) = pinned.clone() {
            self.pinned_dispatch.push(pinned);
        }
        let result = self.function(&self.module.functions[index], arguments);
        if pinned.is_some() {
            let _ = self.pinned_dispatch.pop();
        }
        match result {
            Ok(Flow::Return(value)) => Ok(value.unwrap_or(Value::Null)),
            Ok(Flow::Stop(code)) => {
                self.stop_code = Some(code);
                Ok(Value::Null)
            }
            Err(error) => {
                if let Some(handle) = constructed {
                    let _ = self.objects.delete(handle, span);
                }
                Err(error)
            }
        }
    }

    fn is_bndata_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bndata_providers)
    }

    fn is_bnmath_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bnmath_providers)
    }

    fn standard_provider(name: &str, providers: &HashSet<ModuleId>) -> bool {
        let Some(name) = name.strip_prefix('#') else {
            return false;
        };
        let Some((module, _)) = name.split_once('.') else {
            return false;
        };
        let Ok(id) = module.parse() else {
            return false;
        };
        providers.contains(&ModuleId(id))
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::too_many_lines
    )]
    fn host_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name {
            "HOST.Clock.Timestamp" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(self.host.timestamp_ms()),
                    IntegerType::Int64,
                ))
            }
            "HOST.Clock.Monotonic" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(self.host.monotonic_ns()),
                    IntegerType::Int64,
                ))
            }
            "HOST.Random.Random" => {
                require_arity(name, arguments, 0, span)?;
                let mut state = self.host.random_state.get();
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                state = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
                self.host.random_state.set(state);
                Ok(Value::Float(
                    (state >> 11) as f64 / 9_007_199_254_740_992.0,
                    FloatType::Float64,
                ))
            }
            "HOST.Random.Seed" => {
                require_arity(name, arguments, 1, span)?;
                let (seed, _) = integer(&arguments[0], span)?;
                self.host
                    .random_state
                    .set(seed as u64 | u64::from(seed == 0));
                Ok(Value::Null)
            }
            "HOST.Console.Cls" => {
                require_arity(name, arguments, 0, span)?;
                write!(self.output, "\x1b[2J\x1b[H")
                    .map_err(|e| runtime_error("OUTPUT_ERROR", e.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Console.Beep" => {
                require_arity(name, arguments, 0, span)?;
                write!(self.output, "\x07")
                    .map_err(|e| runtime_error("OUTPUT_ERROR", e.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Console.PrintAt" => {
                require_arity(name, arguments, 3, span)?;
                if !std::io::stdout().is_terminal() {
                    return Err(runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "PrintAt requires a TTY",
                        span,
                    ));
                }
                let (column, _) = integer(&arguments[0], span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(text) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "PrintAt expects STRING",
                        span,
                    ));
                };
                let (cols, rows) = terminal_dimensions().ok_or_else(|| {
                    runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "terminal dimensions are unavailable",
                        span,
                    )
                })?;
                let width = i128::try_from(text.chars().count()).unwrap_or(i128::MAX);
                if column < 1 || row < 1 || row > rows || column > cols || width > cols - column + 1
                {
                    return Err(runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        "console coordinate is outside the window",
                        span,
                    ));
                }
                write!(self.output, "\x1b[{row};{column}H{text}")
                    .map_err(|e| runtime_error("OUTPUT_ERROR", e.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Console.NumCols" | "HOST.Console.NumRows" => {
                require_arity(name, arguments, 0, span)?;
                if !std::io::stdout().is_terminal() {
                    return Err(runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "window size requires a TTY",
                        span,
                    ));
                }
                let (cols, rows) = terminal_dimensions().ok_or_else(|| {
                    runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "terminal dimensions are unavailable",
                        span,
                    )
                })?;
                let value = if name.ends_with("NumCols") {
                    cols
                } else {
                    rows
                };
                integer_from_i128_count(value, span)
            }
            "HOST.FileSystem.Exists" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Exists expects STRING",
                        span,
                    ));
                };
                match std::fs::metadata(path) {
                    Ok(meta) => Ok(Value::Boolean(meta.is_file())),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(Value::Boolean(false))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.FileSystem.Open" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Open expects STRING path",
                        span,
                    ));
                };
                let (mode, _) = integer(&arguments[1], span)?;
                if std::fs::metadata(path).is_ok_and(|meta| meta.is_dir()) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "path is a directory".into(),
                    });
                }
                let result = match mode {
                    0 => std::fs::OpenOptions::new().read(true).open(path),
                    1 => std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(path),
                    2 => std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path),
                    _ => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "unknown file mode".into(),
                        });
                    }
                };
                match result {
                    Ok(file) => {
                        let id = self.next_file;
                        self.next_file += 1;
                        self.files.insert(
                            id,
                            FileResource {
                                file: Some(file),
                                family: None,
                            },
                        );
                        Ok(Value::File(id))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.FileSystem.DeleteFile" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "DeleteFile expects STRING",
                        span,
                    ));
                };
                match std::fs::remove_file(path) {
                    Ok(()) => Ok(Value::Null),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            _ => Err(runtime_error(
                "HOST_CAPABILITY_UNAVAILABLE",
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss
    )]
    fn file_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::File(id) = arguments
            .first()
            .ok_or_else(|| runtime_error("TYPE_MISMATCH", "file receiver missing", span))?
        else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "receiver is not FS.File",
                span,
            ));
        };
        match name.rsplit('.').next().unwrap_or_default() {
            "ReadBytes" => return self.file_read_bytes(*id, arguments, span),
            "WriteBytes" => return self.file_write_bytes(*id, arguments, span),
            _ => {}
        }
        let resource = self
            .files
            .get_mut(id)
            .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "file handle is invalid", span))?;
        match name.rsplit('.').next().unwrap_or_default() {
            "Close" => {
                require_arity(name, arguments, 1, span)?;
                let Some(file) = resource.file.take() else {
                    return Ok(Value::Null);
                };
                resource.family = None;
                match file.sync_all() {
                    Ok(()) => Ok(Value::Null),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "ReadAll" => {
                require_arity(name, arguments, 1, span)?;
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let mut text = String::new();
                match file.read_to_string(&mut text) {
                    Ok(_) => {
                        resource.family = Some(true);
                        Ok(Value::String(text))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "ReadLine" => {
                require_arity(name, arguments, 1, span)?;
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                let mut bytes = Vec::new();
                let mut read_any = false;
                let mut one = [0_u8; 1];
                loop {
                    match file.read(&mut one) {
                        Ok(0) => break,
                        Ok(_) if one[0] == b'\n' => {
                            read_any = true;
                            break;
                        }
                        Ok(_) => {
                            read_any = true;
                            bytes.push(one[0]);
                        }
                        Err(error) => {
                            return Ok(Value::Error {
                                code: 1,
                                message: error.to_string(),
                            });
                        }
                    }
                }
                if !read_any {
                    resource.family = Some(true);
                    return Ok(Value::EndOfFile);
                }
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                Ok(String::from_utf8(bytes).map_or_else(
                    |error| Value::Error {
                        code: 1,
                        message: format!("INVALID_UTF8: {error}"),
                    },
                    |text| {
                        resource.family = Some(true);
                        Value::String(text)
                    },
                ))
            }
            "Write" | "WriteLine" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(text) = &arguments[1] else {
                    return Err(runtime_error("TYPE_MISMATCH", "Write expects STRING", span));
                };
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let text = if name.ends_with("WriteLine") {
                    format!("{text}\n")
                } else {
                    text.clone()
                };
                match file.write_all(text.as_bytes()) {
                    Ok(()) => {
                        resource.family = Some(true);
                        Ok(Value::Null)
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                "unknown FS.File method",
                span,
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn data_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name.rsplit('.').next().unwrap_or_default() {
            "ReadCSV" => {
                require_arity(name, arguments, 3, span)?;
                let Value::Boolean(has_header) = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "ReadCSV expects BOOLEAN header flag",
                        span,
                    ));
                };
                let Value::String(separator) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "ReadCSV expects STRING separator",
                        span,
                    ));
                };
                let separator = separator.chars().collect::<Vec<_>>();
                if separator.len() != 1
                    || separator[0] == '"'
                    || separator[0] == '\n'
                    || separator[0] == '\r'
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid CSV separator".into(),
                    });
                }
                let text = match self.file_call("FS.File.ReadAll", &arguments[..1], span)? {
                    Value::String(text) => text,
                    Value::Error { message, .. } => return Ok(Value::Error { code: 1, message }),
                    _ => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "CSV read failed".into(),
                        });
                    }
                };
                let mut rows = match parse_csv(&text, separator[0]) {
                    Ok(rows) => rows,
                    Err(message) => {
                        return Ok(Value::Error { code: 1, message });
                    }
                };
                let headers = if has_header && !rows.is_empty() {
                    rows.remove(0)
                } else {
                    Vec::new()
                };
                let width = headers.len().max(rows.first().map_or(0, Vec::len));
                if rows.iter().any(|row| row.len() != width)
                    || (has_header && headers.len() != width)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "ragged CSV row".into(),
                    });
                }
                let columns: Vec<DataFrameColumn> = (0..width)
                    .map(|index| DataFrameColumn {
                        name: headers
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("Column{}", index + 1)),
                        values: rows
                            .iter()
                            .map(|row| Value::String(row[index].clone()))
                            .collect(),
                    })
                    .collect();
                if duplicate_column_names(&columns) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "duplicate column name".into(),
                    });
                }
                let id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes.insert(id, DataFrameResource { columns });
                Ok(Value::DataFrame(id))
            }
            "WriteCSV" => {
                require_arity(name, arguments, 4, span)?;
                let Value::File(_) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects FS.File",
                        span,
                    ));
                };
                let Value::DataFrame(id) = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects DataFrame",
                        span,
                    ));
                };
                let Value::Boolean(write_header) = arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects BOOLEAN header flag",
                        span,
                    ));
                };
                let Value::String(separator) = &arguments[3] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects STRING separator",
                        span,
                    ));
                };
                let separator = separator.chars().collect::<Vec<_>>();
                if separator.len() != 1
                    || separator[0] == '"'
                    || separator[0] == '\n'
                    || separator[0] == '\r'
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid CSV separator".into(),
                    });
                }
                let quote = |value: &Value| {
                    let text = render(value);
                    if text.contains([separator[0], '"', '\n', '\r']) {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    } else {
                        text
                    }
                };
                let lines = {
                    let frame = self.dataframes.get(&id).ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
                    })?;
                    let mut lines = Vec::new();
                    if write_header {
                        lines.push(
                            frame
                                .columns
                                .iter()
                                .map(|column| quote(&Value::String(column.name.clone())))
                                .collect::<Vec<_>>()
                                .join(&separator[0].to_string()),
                        );
                    }
                    let rows = frame
                        .columns
                        .first()
                        .map_or(0, |column| column.values.len());
                    lines.extend((0..rows).map(|row| {
                        frame
                            .columns
                            .iter()
                            .map(|column| quote(&column.values[row]))
                            .collect::<Vec<_>>()
                            .join(&separator[0].to_string())
                    }));
                    lines
                };
                let body = if lines.is_empty() {
                    String::new()
                } else {
                    let mut body = lines.join("\n");
                    body.push('\n');
                    body
                };
                match self.file_call(
                    "FS.File.Write",
                    &[arguments[0].clone(), Value::String(body)],
                    span,
                )? {
                    Value::Error { code, message } => Ok(Value::Error { code, message }),
                    _ => Ok(Value::Null),
                }
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                "unknown BNData function",
                span,
            )),
        }
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss
    )]
    fn dataframe_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::DataFrame(id) = arguments
            .first()
            .cloned()
            .ok_or_else(|| runtime_error("TYPE_MISMATCH", "DataFrame receiver missing", span))?
        else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "receiver is not DataFrame",
                span,
            ));
        };
        let method = name.rsplit('.').next().unwrap_or_default();
        if let Some(kind) = match method {
            "Join" => Some(DataFrameJoin::Inner),
            "LeftJoin" => Some(DataFrameJoin::Left),
            "RightJoin" => Some(DataFrameJoin::Right),
            "FullJoin" => Some(DataFrameJoin::Full),
            _ => None,
        } {
            require_arity(name, arguments, 4, span)?;
            let Value::DataFrame(other_id) = arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "join expects DataFrame",
                    span,
                ));
            };
            let Value::String(left_label) = &arguments[2] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "left key must be STRING",
                    span,
                ));
            };
            let Value::String(right_label) = &arguments[3] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "right key must be STRING",
                    span,
                ));
            };
            let left = self.dataframes.get(&id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            let right = self.dataframes.get(&other_id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            let frame = match join_dataframes(left, right, left_label, right_label, kind, equals) {
                Ok(frame) => frame,
                Err(message) => return Ok(Value::Error { code: 1, message }),
            };
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes.insert(new_id, frame);
            return Ok(Value::DataFrame(new_id));
        }
        if method == "AppendRows" || method == "AppendColumns" {
            require_arity(name, arguments, 2, span)?;
            let Value::DataFrame(other_id) = arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "AppendRows/AppendColumns expects DataFrame",
                    span,
                ));
            };
            let left = self.dataframes.get(&id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            let right = self.dataframes.get(&other_id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            if method == "AppendRows" {
                if left.columns.len() != right.columns.len()
                    || left
                        .columns
                        .iter()
                        .zip(&right.columns)
                        .any(|(left, right)| left.name != right.name)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column layouts differ".into(),
                    });
                }
                let mut columns = left.columns.clone();
                for (column, other) in columns.iter_mut().zip(&right.columns) {
                    let left_type = column
                        .values
                        .iter()
                        .find(|value| !matches!(value, Value::NotAvailable))
                        .map(std::mem::discriminant);
                    let right_type = other
                        .values
                        .iter()
                        .find(|value| !matches!(value, Value::NotAvailable))
                        .map(std::mem::discriminant);
                    if left_type.is_some() && right_type.is_some() && left_type != right_type {
                        return Ok(Value::Error {
                            code: 1,
                            message: "column types differ".into(),
                        });
                    }
                    column.values.extend(other.values.clone());
                }
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes
                    .insert(new_id, DataFrameResource { columns });
                return Ok(Value::DataFrame(new_id));
            }
            let rows = left.columns.first().map_or(0, |column| column.values.len());
            if rows
                != right
                    .columns
                    .first()
                    .map_or(0, |column| column.values.len())
            {
                return Ok(Value::Error {
                    code: 1,
                    message: "row counts differ".into(),
                });
            }
            if left.columns.iter().any(|left_column| {
                right
                    .columns
                    .iter()
                    .any(|right_column| left_column.name == right_column.name)
            }) {
                return Ok(Value::Error {
                    code: 1,
                    message: "duplicate column label".into(),
                });
            }
            let mut columns = left.columns.clone();
            columns.extend(right.columns.clone());
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes
                .insert(new_id, DataFrameResource { columns });
            return Ok(Value::DataFrame(new_id));
        }
        let frame = self.dataframes.get_mut(&id).ok_or_else(|| {
            runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
        })?;
        match method {
            "RowCount" => {
                require_arity(name, arguments, 1, span)?;
                integer_from_count(
                    frame
                        .columns
                        .first()
                        .map_or(0, |column| column.values.len()),
                    span,
                )
            }
            "ColumnCount" => {
                require_arity(name, arguments, 1, span)?;
                integer_from_count(frame.columns.len(), span)
            }
            "ColumnName" => {
                require_arity(name, arguments, 2, span)?;
                let (index, _) = integer(&arguments[1], span)?;
                usize::try_from(index)
                    .ok()
                    .and_then(|index| frame.columns.get(index))
                    .map_or_else(
                        || {
                            Ok(Value::Error {
                                code: 1,
                                message: "column index out of bounds".into(),
                            })
                        },
                        |column| Ok(Value::String(column.name.clone())),
                    )
            }
            "SetLabel" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(old_label) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "old label must be STRING",
                        span,
                    ));
                };
                let Value::String(new_label) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "new label must be STRING",
                        span,
                    ));
                };
                if new_label.is_empty()
                    || frame
                        .columns
                        .iter()
                        .any(|column| column.name == *new_label && column.name != *old_label)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid or duplicate column label".into(),
                    });
                }
                let Some(column) = frame
                    .columns
                    .iter_mut()
                    .find(|column| column.name == *old_label)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                column.name.clone_from(new_label);
                Ok(Value::Null)
            }
            "Transpose" => {
                require_arity(name, arguments, 1, span)?;
                let source = frame.columns.clone();
                let rows = source.first().map_or(0, |column| column.values.len());
                let mut columns = Vec::with_capacity(rows + 1);
                columns.push(DataFrameColumn {
                    name: "Column".into(),
                    values: source
                        .iter()
                        .map(|column| Value::String(column.name.clone()))
                        .collect(),
                });
                for row in 0..rows {
                    columns.push(DataFrameColumn {
                        name: format!("Row{row}"),
                        values: source
                            .iter()
                            .map(|column| Value::String(render(&column.values[row])))
                            .collect(),
                    });
                }
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes
                    .insert(new_id, DataFrameResource { columns });
                Ok(Value::DataFrame(new_id))
            }
            "GetString" | "GetInteger" | "GetFloat" | "GetBoolean" => {
                require_arity(name, arguments, 3, span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(column_name) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let Some(value) = usize::try_from(row)
                    .ok()
                    .and_then(|row| column.values.get(row))
                    .cloned()
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "row index out of bounds".into(),
                    });
                };
                match method {
                    "GetString" if matches!(value, Value::String(_) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    "GetInteger" if matches!(value, Value::Integer(_, _) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    "GetFloat" if matches!(value, Value::Float(_, _) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    "GetBoolean" if matches!(value, Value::Boolean(_) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    _ => Ok(Value::Error {
                        code: 1,
                        message: "column type mismatch".into(),
                    }),
                }
            }
            "ConvertToInteger" | "ConvertToFloat" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Some(index) = frame
                    .columns
                    .iter()
                    .position(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let source = &frame.columns[index].values;
                let converted = source
                    .iter()
                    .map(|value| {
                        let Value::String(text) = value else {
                            return Err("column is not STRING");
                        };
                        if text.trim().is_empty() {
                            return Ok(Value::NotAvailable);
                        }
                        let number = parse_val(text);
                        if method == "ConvertToFloat" {
                            Ok(Value::Float(number, FloatType::Float64))
                        } else if number.is_finite()
                            && number.trunc() >= f64::from(i32::MIN)
                            && number.trunc() <= f64::from(i32::MAX)
                        {
                            Ok(Value::Integer(number.trunc() as i128, IntegerType::Int32))
                        } else {
                            Err("integer conversion overflow")
                        }
                    })
                    .collect::<Result<Vec<_>, &str>>();
                let Ok(converted) = converted else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column conversion failed".into(),
                    });
                };
                frame.columns[index].values = converted;
                Ok(Value::Null)
            }
            "ZScore" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let frame = self.dataframes.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
                })?;
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let label = column.name.clone();
                let cells = column.values.clone();
                let numeric = match dataframe_numeric_values(column) {
                    Ok(values) => values,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                };
                let mean = match builtin(
                    "BNMath.MEAN",
                    &[Value::Vector(numeric.clone())],
                    span,
                    &self.memory,
                )? {
                    Value::Float(value, _) => value,
                    _ => f64::NAN,
                };
                let stdev = match builtin(
                    "BNMath.STDEV",
                    &[Value::Vector(numeric)],
                    span,
                    &self.memory,
                )? {
                    Value::Float(value, _) => value,
                    _ => f64::NAN,
                };
                let zscore = |x: f64| {
                    if !stdev.is_finite() || stdev == 0.0 {
                        f64::NAN
                    } else {
                        (x - mean) / stdev
                    }
                };
                let values = cells
                    .into_iter()
                    .map(|value| match value {
                        Value::Integer(number, _) => {
                            Value::Float(zscore(number as f64), FloatType::Float64)
                        }
                        Value::Float(number, _) => Value::Float(zscore(number), FloatType::Float64),
                        _ => Value::NotAvailable,
                    })
                    .collect();
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes.insert(
                    new_id,
                    DataFrameResource {
                        columns: vec![DataFrameColumn {
                            name: label,
                            values,
                        }],
                    },
                );
                Ok(Value::DataFrame(new_id))
            }
            "Mean" | "Median" | "Quartile1" | "Quartile3" | "Mode" | "Stdev" | "Variance"
            | "Range" | "Min" | "Max" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let values = match dataframe_numeric_values(column) {
                    Ok(values) => values,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                };
                if matches!(method, "Min" | "Max") && values.is_empty() {
                    return Ok(Value::Error {
                        code: 1,
                        message: "empty numeric column".into(),
                    });
                }
                let math_name = match method {
                    "Quartile1" => "QUARTILE1".to_string(),
                    "Quartile3" => "QUARTILE3".to_string(),
                    "Stdev" => "STDEV".to_string(),
                    "Variance" => "VARIANCE".to_string(),
                    other => other.to_ascii_uppercase(),
                };
                builtin(
                    &format!("BNMath.{math_name}"),
                    &[Value::Vector(values)],
                    span,
                    &self.memory,
                )
            }
            "CopyIntegerColumn" | "CopyFloatColumn" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "destination must be a pointer",
                        span,
                    ));
                };
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                if self.memory.len(handle, span)? != column.values.len() {
                    return Ok(Value::Error {
                        code: 1,
                        message: "destination length mismatch".into(),
                    });
                }
                let values = column
                    .values
                    .iter()
                    .cloned()
                    .map(|value| match (method, value) {
                        ("CopyIntegerColumn", Value::Integer(number, _)) => {
                            Ok(Value::Integer(number, IntegerType::Int32))
                        }
                        ("CopyFloatColumn", Value::Float(number, _)) => {
                            Ok(Value::Float(number, FloatType::Float64))
                        }
                        _ => Err(()),
                    })
                    .collect::<Result<Vec<_>, _>>();
                let Ok(values) = values else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column type or NA mismatch".into(),
                    });
                };
                for (index, stored) in values.into_iter().enumerate() {
                    *self.memory.get_mut(handle, index, span)? = stored;
                }
                Ok(Value::Null)
            }
            "Select" | "Slice" => {
                let (row_indices, column_indices) = if method == "Select" {
                    require_arity(name, arguments, 3, span)?;
                    let Some(row_indices) =
                        unsigned_indices(collect_indices(&arguments[1], &self.memory, span)?)
                    else {
                        return Ok(dataframe_index_error());
                    };
                    let Some(column_indices) =
                        unsigned_indices(collect_indices(&arguments[2], &self.memory, span)?)
                    else {
                        return Ok(dataframe_index_error());
                    };
                    (row_indices, column_indices)
                } else {
                    require_arity(name, arguments, 5, span)?;
                    let (start_row, _) = integer(&arguments[1], span)?;
                    let (row_count, _) = integer(&arguments[2], span)?;
                    let (start_col, _) = integer(&arguments[3], span)?;
                    let (col_count, _) = integer(&arguments[4], span)?;
                    let values = [start_row, row_count, start_col, col_count]
                        .into_iter()
                        .map(|value| usize::try_from(value).ok())
                        .collect::<Option<Vec<_>>>();
                    let Some(values) = values else {
                        return Ok(Value::Error {
                            code: 1,
                            message: "negative slice bound".into(),
                        });
                    };
                    (
                        (values[0]..values[0].saturating_add(values[1])).collect(),
                        (values[2]..values[2].saturating_add(values[3])).collect(),
                    )
                };
                let source = frame.columns.clone();
                let row_count = source.first().map_or(0, |column| column.values.len());
                if row_indices.iter().any(|row| *row >= row_count)
                    || column_indices.iter().any(|column| *column >= source.len())
                {
                    return Ok(dataframe_index_error());
                }
                let columns: Vec<DataFrameColumn> = column_indices
                    .into_iter()
                    .map(|column| {
                        let source = &source[column];
                        DataFrameColumn {
                            name: source.name.clone(),
                            values: row_indices
                                .iter()
                                .map(|row| source.values[*row].clone())
                                .collect(),
                        }
                    })
                    .collect();
                if duplicate_column_names(&columns) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "duplicate column name".into(),
                    });
                }
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes
                    .insert(new_id, DataFrameResource { columns });
                Ok(Value::DataFrame(new_id))
            }
            "AddIntegerColumn" | "AddFloatColumn" | "AddStringColumn" | "AddBooleanColumn" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                if frame
                    .columns
                    .iter()
                    .any(|column| column.name == *column_name)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "duplicate column name".into(),
                    });
                }
                let values = match &arguments[2] {
                    Value::Vector(values) => values.clone(),
                    Value::Pointer { handle } => (0..self.memory.len(*handle, span)?)
                        .map(|index| self.memory.get(*handle, index, span).cloned())
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "column values must be a vector",
                            span,
                        ));
                    }
                };
                let type_ok = values.iter().all(|value| match method {
                    "AddIntegerColumn" => matches!(value, Value::Integer(_, _)),
                    "AddFloatColumn" => matches!(value, Value::Float(_, _)),
                    "AddStringColumn" => matches!(value, Value::String(_)),
                    "AddBooleanColumn" => matches!(value, Value::Boolean(_)),
                    _ => false,
                });
                if !type_ok {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column type mismatch".into(),
                    });
                }
                if frame
                    .columns
                    .first()
                    .is_some_and(|column| column.values.len() != values.len())
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column length mismatch".into(),
                    });
                }
                frame.columns.push(DataFrameColumn {
                    name: column_name.clone(),
                    values,
                });
                Ok(Value::Null)
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                "unknown DataFrame method",
                span,
            )),
        }
    }

    fn file_read_bytes(
        &mut self,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("FS.File.ReadBytes", arguments, 2, span)?;
        let Value::Pointer { handle } = arguments[1] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "ReadBytes expects BYTE buffer",
                span,
            ));
        };
        let (bytes, eof) = {
            let resource = self
                .files
                .get_mut(&id)
                .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "file handle is invalid", span))?;
            if resource.family == Some(true) {
                return Ok(Value::Error {
                    code: 1,
                    message: "file is in text mode".into(),
                });
            }
            let Some(file) = resource.file.as_mut() else {
                return Ok(Value::Error {
                    code: 1,
                    message: "file is closed".into(),
                });
            };
            let len = self.memory.len(handle, span)?;
            let mut bytes = vec![0; len];
            let count = match file.read(&mut bytes) {
                Ok(count) => count,
                Err(error) => {
                    return Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    });
                }
            };
            resource.family = Some(false);
            (bytes, count)
        };
        if eof == 0 {
            return Ok(Value::EndOfFile);
        }
        for (index, byte) in bytes.into_iter().take(eof).enumerate() {
            *self.memory.get_mut(handle, index, span)? =
                Value::Integer(i128::from(byte), IntegerType::Byte);
        }
        Ok(Value::Integer(
            i128::try_from(eof).unwrap_or(i128::MAX),
            IntegerType::Int32,
        ))
    }

    fn file_write_bytes(
        &mut self,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("FS.File.WriteBytes", arguments, 3, span)?;
        let Value::Pointer { handle } = arguments[1] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "WriteBytes expects BYTE buffer",
                span,
            ));
        };
        let (count, _) = integer(&arguments[2], span)?;
        let len = self.memory.len(handle, span)?;
        let count = usize::try_from(count)
            .ok()
            .filter(|count| *count <= len)
            .ok_or_else(|| {
                runtime_error(
                    "INDEX_OUT_OF_BOUNDS",
                    "byte count exceeds buffer length",
                    span,
                )
            })?;
        let bytes = (0..count)
            .map(|index| {
                let value = self.memory.get(handle, index, span)?;
                let (value, _) = integer(value, span)?;
                u8::try_from(value)
                    .map_err(|_| runtime_error("TYPE_MISMATCH", "buffer is not BYTE", span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let resource = self
            .files
            .get_mut(&id)
            .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "file handle is invalid", span))?;
        if resource.family == Some(true) {
            return Ok(Value::Error {
                code: 1,
                message: "file is in text mode".into(),
            });
        }
        let Some(file) = resource.file.as_mut() else {
            return Ok(Value::Error {
                code: 1,
                message: "file is closed".into(),
            });
        };
        if let Err(error) = file.write_all(&bytes) {
            return Ok(Value::Error {
                code: 1,
                message: error.to_string(),
            });
        }
        resource.family = Some(false);
        Ok(Value::Null)
    }

    fn dispatch_name(&self, name: &str, arguments: &[Value]) -> String {
        if name.ends_with(".$fields")
            || name.ends_with(".CONSTRUCTOR")
            || name.ends_with(".DESTRUCTOR")
        {
            return name.to_string();
        }
        let Some(Value::Object { class, .. }) = arguments.first() else {
            return name.to_string();
        };
        let Some(method) = name.rsplit('.').next() else {
            return name.to_string();
        };
        let dispatched = arguments
            .first()
            .and_then(|value| match value {
                Value::Object { handle, .. } => self
                    .pinned_dispatch
                    .iter()
                    .rev()
                    .find(|(pinned, _)| pinned == handle)
                    .map(|(_, class)| format!("{class}.{method}")),
                _ => None,
            })
            .unwrap_or_else(|| format!("{class}.{method}"));
        if self
            .module
            .functions
            .iter()
            .any(|function| function.name == dispatched)
        {
            dispatched
        } else {
            name.to_string()
        }
    }

    fn allocate_object(&mut self, class: &str, span: Span) -> Result<Value, Diagnostic> {
        let handle = self.objects.allocate(
            class,
            1,
            Instance {
                fields: HashMap::new(),
            },
            span,
        )?;
        Ok(Value::Object {
            handle,
            class: class.to_string(),
        })
    }

    fn allocate_region(
        &mut self,
        element: &Type,
        arguments: &[ValueId],
        values: &HashMap<ValueId, Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let count = if let Some(argument) = arguments.first() {
            let (count, _) = integer(value(values, *argument, span)?, span)?;
            if count < 0 {
                return Err(runtime_error(
                    "ALLOCATION_SIZE_INVALID",
                    "numeric NEW length cannot be negative",
                    span,
                ));
            }
            usize::try_from(count).map_err(|_| {
                runtime_error(
                    "ALLOCATION_SIZE_OVERFLOW",
                    "allocation length does not fit the host",
                    span,
                )
            })?
        } else {
            1
        };
        let element_size = pointer_element_size(element).ok_or_else(|| {
            runtime_error(
                "TYPE_MISMATCH",
                "pointer element is not a numeric type",
                span,
            )
        })?;
        let bytes = u64::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(element_size))
            .ok_or_else(|| {
                runtime_error(
                    "ALLOCATION_SIZE_OVERFLOW",
                    "allocation byte size overflowed",
                    span,
                )
            })?;
        if bytes > isize::MAX as u64 {
            return Err(runtime_error(
                "ALLOCATION_TOO_LARGE",
                "allocation exceeds the host limit",
                span,
            ));
        }
        let initial = pointer_element_default(element, span)?;
        let handle = self
            .memory
            .allocate(display_element(element), count, initial, span)?;
        Ok(Value::Pointer { handle })
    }

    fn index_value(&self, object: &Value, index: usize, span: Span) -> Result<Value, Diagnostic> {
        match object {
            Value::Null => Err(runtime_error(
                "NULL_POINTER_ACCESS",
                "cannot index a NULL pointer",
                span,
            )),
            Value::Vector(vector) => vector.get(index).cloned().ok_or_else(|| {
                runtime_error(
                    "INDEX_OUT_OF_BOUNDS",
                    format!("index {index} is outside vector length {}", vector.len()),
                    span,
                )
            }),
            Value::Pointer { handle } => self.memory.get(*handle, index, span).cloned(),
            Value::String(text) => text
                .chars()
                .nth(index)
                .map(|character| Value::String(character.into()))
                .ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!(
                            "index {index} is outside string length {}",
                            text.chars().count()
                        ),
                        span,
                    )
                }),
            Value::HostArgs => self
                .host
                .arguments
                .get(index)
                .cloned()
                .map(Value::String)
                .ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!(
                            "index {index} is outside argument count {}",
                            self.host.arguments.len()
                        ),
                        span,
                    )
                }),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value is not indexable",
                span,
            )),
        }
    }

    fn set_index(
        &mut self,
        target: &mut Value,
        indices: &[usize],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some((&index, remaining)) = indices.split_first() else {
            *target = stored;
            return Ok(());
        };
        match target {
            Value::Null => Err(runtime_error(
                "NULL_POINTER_ACCESS",
                "cannot index a NULL pointer",
                span,
            )),
            Value::Pointer { handle } => {
                if !remaining.is_empty() {
                    return Err(runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        "pointer indexing requires one index",
                        span,
                    ));
                }
                *self.memory.get_mut(*handle, index, span)? = stored;
                Ok(())
            }
            Value::Vector(vector) => {
                let length = vector.len();
                let element = vector.get_mut(index).ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!("index {index} is outside vector length {length}"),
                        span,
                    )
                })?;
                self.set_index(element, remaining, stored, span)
            }
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value is not indexable",
                span,
            )),
        }
    }

    fn delete_value(
        &mut self,
        target: Value,
        destructor: Option<&str>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match target {
            Value::Null => Err(runtime_error(
                "NULL_POINTER_ACCESS",
                "cannot DELETE NULL",
                span,
            )),
            Value::Pointer { handle } => self.memory.delete(handle, span),
            Value::Object { handle, .. } => {
                if let Some(destructor) = destructor {
                    self.objects.begin_delete(handle, span)?;
                    let result = self.call_named(destructor, vec![target], span);
                    self.objects.finish_delete(handle, span)?;
                    result.map(|_| ())
                } else {
                    self.objects.delete(handle, span)
                }
            }
            Value::File(id) => {
                if self.files.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "file handle was already deleted",
                        span,
                    ))
                }
            }
            Value::DataFrame(id) => {
                if self.dataframes.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "DataFrame handle was already deleted",
                        span,
                    ))
                }
            }
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "DELETE requires a pointer or CLASS reference",
                span,
            )),
        }
    }

    fn coerce_to(&self, value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
        if let (
            Value::Pointer { handle },
            Type::Pointer {
                length: PointerLength::Fixed(expected),
                ..
            },
        ) = (&value, ty)
        {
            let actual = u64::try_from(self.memory.len(*handle, span)?).map_err(|_| {
                runtime_error(
                    "POINTER_LENGTH_MISMATCH",
                    "pointer length does not fit INTEGER",
                    span,
                )
            })?;
            if actual != *expected {
                return Err(runtime_error(
                    "POINTER_LENGTH_MISMATCH",
                    format!("pointer length {actual} does not match {expected}"),
                    span,
                ));
            }
        }
        coerce(value, ty, span)
    }

    fn member_of(&self, object: &Value, name: &str, span: Span) -> Result<Value, Diagnostic> {
        match (object, name) {
            (Value::Error { code, .. }, "Code") => {
                Ok(Value::Integer(i128::from(*code), IntegerType::Int32))
            }
            (Value::Error { message, .. }, "Message") => Ok(Value::String(message.clone())),
            (Value::Record { fields, .. }, _) => fields.get(name).cloned().ok_or_else(|| {
                runtime_error(
                    "NAME_NOT_FOUND",
                    format!("runtime value has no member '{name}'"),
                    span,
                )
            }),
            (Value::Object { handle, .. }, _) => {
                let instance = self.objects.get(*handle, 0, span)?;
                instance.fields.get(name).cloned().ok_or_else(|| {
                    runtime_error(
                        "NAME_NOT_FOUND",
                        format!("runtime value has no member '{name}'"),
                        span,
                    )
                })
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("runtime value has no member '{name}'"),
                span,
            )),
        }
    }

    fn set_member_value(
        &mut self,
        values: &mut HashMap<ValueId, Value>,
        object: ValueId,
        name: &str,
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match values.get(&object) {
            Some(Value::Object { handle, .. }) => {
                let handle = *handle;
                self.objects
                    .get_mut(handle, 0, span)?
                    .fields
                    .insert(name.to_string(), stored);
                Ok(())
            }
            Some(Value::Record { .. }) => {
                let Some(Value::Record { fields, .. }) = values.get_mut(&object) else {
                    return Err(runtime_error(
                        "INVALID_IR",
                        "record value disappeared",
                        span,
                    ));
                };
                fields.insert(name.to_string(), stored);
                Ok(())
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("runtime value has no member '{name}'"),
                span,
            )),
        }
    }

    fn set_field_path(
        &mut self,
        target: &mut Value,
        path: &[String],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some((name, rest)) = path.split_first() else {
            *target = stored;
            return Ok(());
        };
        match target {
            Value::Record { fields, .. } => {
                if rest.is_empty() {
                    fields.insert(name.clone(), stored);
                    return Ok(());
                }
                let mut nested = fields.get(name).cloned().ok_or_else(|| {
                    runtime_error(
                        "NAME_NOT_FOUND",
                        format!("runtime value has no member '{name}'"),
                        span,
                    )
                })?;
                self.set_field_path(&mut nested, rest, stored, span)?;
                fields.insert(name.clone(), nested);
                Ok(())
            }
            Value::Object { handle, .. } => {
                let handle = *handle;
                if rest.is_empty() {
                    self.objects
                        .get_mut(handle, 0, span)?
                        .fields
                        .insert(name.clone(), stored);
                    return Ok(());
                }
                let mut nested = self
                    .objects
                    .get(handle, 0, span)?
                    .fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| {
                        runtime_error(
                            "NAME_NOT_FOUND",
                            format!("runtime value has no member '{name}'"),
                            span,
                        )
                    })?;
                self.set_field_path(&mut nested, rest, stored, span)?;
                self.objects
                    .get_mut(handle, 0, span)?
                    .fields
                    .insert(name.clone(), nested);
                Ok(())
            }
            _ => Err(runtime_error("TYPE_MISMATCH", "value has no fields", span)),
        }
    }

    fn default_value(
        &mut self,
        ty: &Type,
        dimensions: &[usize],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match ty {
            Type::Boolean => Ok(Value::Boolean(false)),
            Type::Integer(kind) => Ok(Value::Integer(0, *kind)),
            Type::Float(kind) => Ok(Value::Float(0.0, *kind)),
            Type::String => Ok(Value::String(String::new())),
            Type::Vector {
                element,
                dimensions: declared_dimensions,
            } => {
                let owned_dimensions;
                let dimensions = if dimensions.is_empty() {
                    owned_dimensions = declared_dimensions
                        .iter()
                        .map(|dimension| usize::try_from(*dimension))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| {
                            runtime_error("INVALID_IR", "vector dimension is too large", span)
                        })?;
                    &owned_dimensions
                } else {
                    dimensions
                };
                let Some(_) = dimensions.first() else {
                    return Err(runtime_error(
                        "INVALID_IR",
                        "vector default is missing its dimension",
                        span,
                    ));
                };
                let mut value = if let Some(name) = default_function_owner(element) {
                    self.default_named(&name, span)?
                } else {
                    self.default_value(element, &[], span)?
                };
                for length in dimensions.iter().rev() {
                    value = Value::Vector(vec![value; *length]);
                }
                Ok(value)
            }
            Type::Alternative(types) => self.default_value(
                types
                    .first()
                    .ok_or_else(|| runtime_error("INVALID_IR", "empty alternative type", span))?,
                dimensions,
                span,
            ),
            Type::Named(name) | Type::TypeName(name) => Ok(empty_named(name)),
            Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
                Ok(empty_named(&format!("#{}.{name}", module.0)))
            }
            Type::System => Ok(Value::Type("SYSTEM".into())),
            Type::HostClock => Ok(Value::Type("HOST.Clock".into())),
            Type::HostRandom => Ok(Value::Type("HOST.Random".into())),
            Type::HostFileSystem => Ok(Value::Type("HOST.FileSystem".into())),
            _ => Err(runtime_error(
                "UNINITIALIZED_VALUE",
                "type has no default value",
                span,
            )),
        }
    }

    fn default_named(&mut self, ir_name: &str, span: Span) -> Result<Value, Diagnostic> {
        let function_name = format!("{ir_name}.$default");
        if self
            .module
            .functions
            .iter()
            .any(|function| function.name == function_name)
        {
            self.call_named(&function_name, Vec::new(), span)
        } else {
            Ok(empty_named(ir_name))
        }
    }

    fn size_of_value(&self, value: &Value, span: Span) -> Result<Value, Diagnostic> {
        let size = match value {
            Value::Integer(_, kind) => integer_byte_size(*kind),
            Value::Float(_, FloatType::Float32) | Value::Date(_) | Value::Time(_) => 4,
            Value::Float(_, FloatType::Float64) => 8,
            Value::Boolean(_) => 1,
            Value::String(text) => u64::try_from(text.len()).map_err(|_| integer_overflow(span))?,
            Value::Vector(elements) => {
                let mut total = 0u64;
                for element in elements {
                    total = add_sizes(total, &self.size_of_value(element, span)?, span)?;
                }
                total
            }
            Value::Record { fields, .. } => {
                let mut total = 0u64;
                for field in fields.values() {
                    total = add_sizes(total, &self.size_of_value(field, span)?, span)?;
                }
                total
            }
            Value::Object { handle, .. } => {
                let instance = self.objects.get(*handle, 0, span)?;
                let mut total = 0u64;
                for field in instance.fields.values() {
                    total = add_sizes(total, &self.size_of_value(field, span)?, span)?;
                }
                total
            }
            _ => {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "value has no byte size",
                    span,
                ));
            }
        };
        integer_from_u64(size, span)
    }
}

#[cfg(all(
    unix,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    )
))]
#[allow(unsafe_code)] // Narrow Unix counterpart of the BDFL-approved Win32 stdout query; ioctl on fd 1 only.
fn terminal_dimensions() -> Option<(i128, i128)> {
    use std::os::raw::{c_int, c_ulong};

    #[repr(C)]
    struct WinSize {
        row: u16,
        col: u16,
        xpixel: u16,
        ypixel: u16,
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    const TIOCGWINSZ: c_ulong = 0x5413;
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    const TIOCGWINSZ: c_ulong = 0x4008_7468;

    unsafe extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }

    let mut size = WinSize {
        row: 0,
        col: 0,
        xpixel: 0,
        ypixel: 0,
    };
    if unsafe { ioctl(1, TIOCGWINSZ, &raw mut size) } != 0 || size.col == 0 || size.row == 0 {
        return None;
    }
    Some((i128::from(size.col), i128::from(size.row)))
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))
))]
fn terminal_dimensions() -> Option<(i128, i128)> {
    None
}

#[cfg(windows)]
#[allow(unsafe_code)] // Narrow BDFL-approved Win32 terminal query; no Rust memory is retained by Windows.
fn terminal_dimensions() -> Option<(i128, i128)> {
    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct SmallRect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }
    #[repr(C)]
    struct ConsoleScreenBufferInfo {
        size: Coord,
        cursor_position: Coord,
        attributes: u16,
        window: SmallRect,
        maximum_window_size: Coord,
    }
    unsafe extern "system" {
        fn GetStdHandle(standard_handle: u32) -> *mut std::ffi::c_void;
        fn GetConsoleScreenBufferInfo(
            console_output: *mut std::ffi::c_void,
            console_screen_buffer_info: *mut ConsoleScreenBufferInfo,
        ) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = u32::MAX - 10;
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if handle.is_null() || handle as isize == -1 {
        return None;
    }
    let mut info = std::mem::MaybeUninit::<ConsoleScreenBufferInfo>::uninit();
    if unsafe { GetConsoleScreenBufferInfo(handle, info.as_mut_ptr()) } == 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some((
        i128::from(info.window.right) - i128::from(info.window.left) + 1,
        i128::from(info.window.bottom) - i128::from(info.window.top) + 1,
    ))
}

#[cfg(not(any(unix, windows)))]
fn terminal_dimensions() -> Option<(i128, i128)> {
    None
}

fn find_block(function: &Function, id: BlockId) -> Result<&BasicBlock, Diagnostic> {
    function
        .blocks
        .get(id.0 as usize)
        .filter(|block| block.id == id)
        .ok_or_else(|| runtime_error("INVALID_IR", "basic block does not exist", function.span))
}

fn set(values: &mut HashMap<ValueId, Value>, destination: ValueId, value: Value) {
    values.insert(destination, value);
}

fn pointer_element_default(element: &Type, span: Span) -> Result<Value, Diagnostic> {
    match element {
        Type::Integer(kind) => Ok(Value::Integer(0, *kind)),
        Type::Float(kind) => Ok(Value::Float(0.0, *kind)),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "pointer element is not a numeric type",
            span,
        )),
    }
}

fn pointer_element_size(element: &Type) -> Option<u64> {
    match element {
        Type::Integer(kind) => Some(integer_byte_size(*kind)),
        Type::Float(FloatType::Float32) => Some(4),
        Type::Float(FloatType::Float64) => Some(8),
        _ => None,
    }
}

fn display_element(element: &Type) -> String {
    match element {
        Type::Integer(kind) => format!("{kind:?}"),
        Type::Float(kind) => format!("{kind:?}"),
        _ => "POINTER".into(),
    }
}

fn value(values: &HashMap<ValueId, Value>, id: ValueId, span: Span) -> Result<&Value, Diagnostic> {
    values
        .get(&id)
        .ok_or_else(|| runtime_error("INVALID_IR", format!("value %{} is undefined", id.0), span))
}

fn constant_value(constant: &Constant, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match constant {
        Constant::Integer(value) => Ok(Value::Integer(
            parse_integer(value)
                .ok_or_else(|| runtime_error("INVALID_IR", "invalid integer constant", span))?,
            integer_kind(ty).unwrap_or(IntegerType::Int32),
        )),
        Constant::Float(value) => Ok(float_value(parse_float(value), float_kind(ty))),
        Constant::String(value) => Ok(Value::String(value.clone())),
        Constant::Boolean(value) => Ok(Value::Boolean(*value)),
        Constant::Null => Ok(Value::Null),
        Constant::NotAvailable => Ok(Value::NotAvailable),
        Constant::EndOfFile => Ok(Value::EndOfFile),
        Constant::Function(value) => Ok(Value::Function(value.clone())),
        Constant::Type(value) => Ok(Value::Type(value.clone())),
        Constant::HostConsole => Ok(Value::HostConsole),
        Constant::HostArgs => Ok(Value::HostArgs),
    }
}

fn empty_named(name: &str) -> Value {
    match name {
        "DATE" => Value::Date(temporal::default_date()),
        "TIME" => Value::Time(temporal::default_time()),
        "TIMEZONE" => Value::TimeZone("UTC".into()),
        "VOID" => Value::Handle {
            type_name: name.into(),
        },
        "Error" => Value::Error {
            code: 0,
            message: String::new(),
        },
        _ => Value::Record {
            type_name: name.into(),
            fields: HashMap::new(),
        },
    }
}

fn require_arity(
    name: &str,
    arguments: &[Value],
    expected: usize,
    span: Span,
) -> Result<(), Diagnostic> {
    if arguments.len() == expected {
        Ok(())
    } else {
        Err(runtime_error(
            "TYPE_MISMATCH",
            format!("{name} expects {expected} argument(s)"),
            span,
        ))
    }
}

fn is_temporal_builtin(name: &str) -> bool {
    matches!(
        name,
        "Date.Parse"
            | "Time.Parse"
            | "TimeZone.Parse"
            | "Timestamp.Parse"
            | "Timestamp.Format"
            | "BNMath.TODATE"
            | "BNMath.TOTIME"
            | "BNMath.TOTIMESTAMP"
    )
}

#[allow(clippy::too_many_lines)] // Temporal builtins share arity and TIMESTAMP range checks.
fn temporal_call(name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
    match name {
        "Date.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "Date.Parse expects STRING",
                    span,
                ));
            };
            Ok(Value::Date(temporal::parse_date(text, span)?))
        }
        "Time.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "Time.Parse expects STRING",
                    span,
                ));
            };
            Ok(Value::Time(temporal::parse_time(text, span)?))
        }
        "TimeZone.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "TimeZone.Parse expects STRING",
                    span,
                ));
            };
            Ok(Value::TimeZone(temporal::parse_timezone(text, span)?))
        }
        "Timestamp.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "Timestamp.Parse expects STRING",
                    span,
                ));
            };
            Ok(Value::Integer(
                i128::from(temporal::parse_rfc3339(text, span)?),
                IntegerType::Int64,
            ))
        }
        "Timestamp.Format" => {
            require_arity(name, arguments, 1, span)?;
            let (timestamp, _) = integer(&arguments[0], span)?;
            let timestamp = i64::try_from(timestamp).map_err(|_| {
                runtime_error(
                    "FORMAT_OUT_OF_RANGE",
                    "TIMESTAMP is outside 0001-01-01..9999-12-31",
                    span,
                )
            })?;
            Ok(Value::String(temporal::format_rfc3339(timestamp, span)?))
        }
        "BNMath.TODATE" => {
            require_arity(name, arguments, 1, span)?;
            let (timestamp, _) = integer(&arguments[0], span)?;
            let timestamp = i64::try_from(timestamp).map_err(|_| {
                runtime_error(
                    "FORMAT_OUT_OF_RANGE",
                    "TIMESTAMP is outside 0001-01-01..9999-12-31",
                    span,
                )
            })?;
            Ok(Value::Date(temporal::date_from_timestamp(timestamp, span)?))
        }
        "BNMath.TOTIME" => {
            require_arity(name, arguments, 1, span)?;
            let (timestamp, _) = integer(&arguments[0], span)?;
            let timestamp = i64::try_from(timestamp).map_err(|_| {
                runtime_error(
                    "FORMAT_OUT_OF_RANGE",
                    "TIMESTAMP is outside 0001-01-01..9999-12-31",
                    span,
                )
            })?;
            Ok(Value::Time(temporal::time_from_timestamp(timestamp, span)?))
        }
        "BNMath.TOTIMESTAMP" => {
            require_arity(name, arguments, 2, span)?;
            let Value::Date(days) = arguments[0] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNMath.TOTIMESTAMP expects DATE and TIME",
                    span,
                ));
            };
            let Value::Time(millis) = arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNMath.TOTIMESTAMP expects DATE and TIME",
                    span,
                ));
            };
            Ok(Value::Integer(
                i128::from(temporal::timestamp_from_date_time(days, millis, span)?),
                IntegerType::Int64,
            ))
        }
        _ => Err(runtime_error(
            "NAME_NOT_FOUND",
            format!("unknown temporal function '{name}'"),
            span,
        )),
    }
}

fn default_function_owner(ty: &Type) -> Option<String> {
    match ty {
        Type::Named(name) | Type::TypeName(name) => Some(name.clone()),
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            Some(format!("#{}.{name}", module.0))
        }
        _ => None,
    }
}

fn add_sizes(total: u64, size: &Value, span: Span) -> Result<u64, Diagnostic> {
    let Value::Integer(size, _) = size else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "value has no byte size",
            span,
        ));
    };
    let size = u64::try_from(*size).map_err(|_| integer_overflow(span))?;
    total
        .checked_add(size)
        .ok_or_else(|| integer_overflow(span))
}

fn unary(operator: &str, operand: &Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match (operator, operand) {
        ("Minus", Value::Integer(value, _)) => checked_integer(value.checked_neg(), ty, span),
        ("Minus", Value::Float(value, _)) => Ok(float_value(-value, float_kind(ty))),
        ("NOT", Value::Boolean(value)) => Ok(Value::Boolean(!value)),
        ("NOT", Value::Integer(value, _)) => checked_integer(Some(!value), ty, span),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid unary operation",
            span,
        )),
    }
}

#[allow(clippy::too_many_lines)] // Operator behavior is intentionally explicit and centralized.
fn binary(
    operator: &str,
    left: &Value,
    right: &Value,
    ty: &Type,
    span: Span,
) -> Result<Value, Diagnostic> {
    if operator == "IS" {
        let Value::Type(test) = right else {
            return Err(runtime_error(
                "INVALID_IR",
                "IS requires a type operand",
                span,
            ));
        };
        return Ok(Value::Boolean(is_value(left, test)));
    }
    if matches!(operator, "Assign" | "NotEqual") {
        let equal = equals(left, right);
        return Ok(Value::Boolean(if operator == "Assign" {
            equal
        } else {
            !equal
        }));
    }
    if let (Value::Boolean(left), Value::Boolean(right)) = (left, right) {
        return match operator {
            "AND" => Ok(Value::Boolean(*left && *right)),
            "OR" => Ok(Value::Boolean(*left || *right)),
            "XOR" => Ok(Value::Boolean(*left ^ *right)),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid BOOLEAN operation",
                span,
            )),
        };
    }
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return match operator {
            "Plus" => Ok(Value::String(format!("{left}{right}"))),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid STRING operation",
                span,
            )),
        };
    }
    if let (Value::Date(left), Value::Date(right)) = (left, right) {
        return ordered(operator, left, right, span);
    }
    if let (Value::Time(left), Value::Time(right)) = (left, right) {
        return ordered(operator, left, right, span);
    }
    if is_float_value(left) || is_float_value(right) || operator == "Slash" {
        let left = number_as_float(left, span)?;
        let right = number_as_float(right, span)?;
        return match operator {
            "Plus" => Ok(float_value(left + right, float_kind(ty))),
            "Minus" => Ok(float_value(left - right, float_kind(ty))),
            "Star" => Ok(float_value(left * right, float_kind(ty))),
            "Slash" => Ok(float_value(left / right, float_kind(ty))),
            "Power" => Ok(float_value(left.powf(right), float_kind(ty))),
            "Less" => Ok(Value::Boolean(left < right)),
            "LessEqual" => Ok(Value::Boolean(left <= right)),
            "Greater" => Ok(Value::Boolean(left > right)),
            "GreaterEqual" => Ok(Value::Boolean(left >= right)),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid floating operation",
                span,
            )),
        };
    }
    let (left, _) = integer(left, span)?;
    let (right, _) = integer(right, span)?;
    match operator {
        "Plus" => checked_integer(left.checked_add(right), ty, span),
        "Minus" => checked_integer(left.checked_sub(right), ty, span),
        "Star" => checked_integer(left.checked_mul(right), ty, span),
        "DIV" if right != 0 => checked_integer(left.checked_div_euclid(right), ty, span),
        "Percent" if right != 0 => checked_integer(left.checked_rem_euclid(right), ty, span),
        "DIV" | "Percent" => Err(runtime_error(
            "DIVISION_BY_ZERO",
            "integer divisor cannot be zero",
            span,
        )),
        "Power" if right >= 0 => checked_integer(
            left.checked_pow(u32::try_from(right).map_err(|_| {
                runtime_error("INVALID_EXPONENT", "integer exponent is too large", span)
            })?),
            ty,
            span,
        ),
        "Power" => Err(runtime_error(
            "INVALID_EXPONENT",
            "integer exponent cannot be negative",
            span,
        )),
        "AND" => checked_integer(Some(left & right), ty, span),
        "OR" => checked_integer(Some(left | right), ty, span),
        "XOR" => checked_integer(Some(left ^ right), ty, span),
        "SHL" => shift(left, right, ty, true, span),
        "SHR" => shift(left, right, ty, false, span),
        "Less" => Ok(Value::Boolean(left < right)),
        "LessEqual" => Ok(Value::Boolean(left <= right)),
        "Greater" => Ok(Value::Boolean(left > right)),
        "GreaterEqual" => Ok(Value::Boolean(left >= right)),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid integer operation",
            span,
        )),
    }
}

fn shift(value: i128, count: i128, ty: &Type, left: bool, span: Span) -> Result<Value, Diagnostic> {
    let width = integer_width(integer_kind(ty).unwrap_or(IntegerType::Int32));
    if count < 0 || count >= i128::from(width) {
        return Err(runtime_error(
            "INVALID_SHIFT_COUNT",
            format!("shift count must be in 0..{width}"),
            span,
        ));
    }
    let count = u32::try_from(count).expect("validated shift count");
    if left {
        checked_integer(value.checked_shl(count), ty, span)
    } else {
        let mask = (1_u128 << width) - 1;
        checked_integer(
            Some(((value.cast_unsigned() & mask) >> count).cast_signed()),
            ty,
            span,
        )
    }
}

fn cast(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match ty {
        Type::Boolean => Ok(Value::Boolean(match value {
            Value::Boolean(value) => value,
            Value::Integer(value, _) => value != 0,
            Value::Float(value, _) => value != 0.0,
            Value::String(value) => !value.is_empty(),
            Value::Null | Value::NotAvailable | Value::EndOfFile => false,
            _ => true,
        })),
        Type::Integer(_) => match value {
            Value::Integer(value, _) => checked_integer(Some(value), ty, span),
            #[allow(clippy::cast_possible_truncation)]
            // BN specifies truncation followed by range checking.
            Value::Float(value, _) if value.is_finite() => {
                checked_integer(Some(value.trunc() as i128), ty, span)
            }
            Value::Float(_, _) => Err(runtime_error(
                "INVALID_NUMERIC_CONVERSION",
                "NAN and infinity cannot convert to an integer",
                span,
            )),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value cannot convert to an integer",
                span,
            )),
        },
        Type::Float(_) => Ok(float_value(number_as_float(&value, span)?, float_kind(ty))),
        Type::Named(_) | Type::ImportedNamed { .. } => match value {
            Value::Object { .. } | Value::Record { .. } | Value::Handle { .. } | Value::Null => {
                Ok(value)
            }
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "unsupported conversion",
                span,
            )),
        },
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "unsupported conversion",
            span,
        )),
    }
}

fn coerce(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match (&value, ty) {
        (Value::Integer(number, _), Type::Integer(_)) => checked_integer(Some(*number), ty, span),
        (Value::Float(number, _), Type::Float(_)) => Ok(float_value(*number, float_kind(ty))),
        (_, Type::Alternative(types)) if types.iter().any(|ty| value_matches_type(&value, ty)) => {
            Ok(value)
        }
        (Value::Boolean(_), Type::Boolean)
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Function(_), Type::Function { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile)
        | (Value::HostConsole, Type::HostConsole)
        | (Value::Handle { .. }, Type::Named(_) | Type::Pointer { .. })
        | (Value::Pointer { .. }, Type::Pointer { .. })
        | (
            Value::Record { .. } | Value::Object { .. },
            Type::Named(_) | Type::ImportedNamed { .. },
        )
        | (
            Value::File(_),
            Type::Named(_)
            | Type::ImportedNamed { .. }
            | Type::TypeName(_)
            | Type::ImportedTypeName { .. },
        )
        | (
            Value::DataFrame(_),
            Type::Named(_)
            | Type::ImportedNamed { .. }
            | Type::TypeName(_)
            | Type::ImportedTypeName { .. },
        )
        | (
            Value::Type(_),
            Type::System | Type::HostClock | Type::HostRandom | Type::HostFileSystem,
        ) => Ok(value),
        (Value::Date(_), Type::Named(name)) if name == "DATE" => Ok(value),
        (Value::Time(_), Type::Named(name)) if name == "TIME" => Ok(value),
        (Value::TimeZone(_), Type::Named(name)) if name == "TIMEZONE" => Ok(value),
        (Value::Null, Type::Named(name)) if name == "VOID" => Ok(value),
        (Value::Error { .. }, Type::Named(name)) if name == "Error" => Ok(value),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "runtime value does not match its IR destination type",
            span,
        )),
    }
}

fn checked_integer(value: Option<i128>, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    let value = value
        .ok_or_else(|| runtime_error("NUMERIC_OVERFLOW", "integer operation overflowed", span))?;
    let kind = integer_kind(ty).unwrap_or(IntegerType::Int32);
    let (minimum, maximum) = integer_range(kind);
    if !(minimum..=maximum).contains(&value) {
        return Err(runtime_error(
            "NUMERIC_OVERFLOW",
            format!("{value} does not fit {kind:?}"),
            span,
        ));
    }
    Ok(Value::Integer(value, kind))
}

#[allow(clippy::too_many_lines)] // Standard numeric functions share argument decoding and errors.
fn builtin(
    name: &str,
    arguments: &[Value],
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    if name == "$for_condition" {
        let current = integer(&arguments[0], span)?.0;
        let end = integer(&arguments[1], span)?.0;
        let step = integer(&arguments[2], span)?.0;
        if step == 0 {
            return Err(runtime_error(
                "INVALID_FOR_STEP",
                "FOR STEP cannot be zero",
                span,
            ));
        }
        return Ok(Value::Boolean(if step > 0 {
            current <= end
        } else {
            current >= end
        }));
    }
    if name == "ASC" {
        let Value::String(text) = &arguments[0] else {
            return Err(runtime_error("TYPE_MISMATCH", "ASC expects STRING", span));
        };
        return Ok(text.chars().next().map_or_else(
            || Value::Error {
                code: 1,
                message: "ASC requires a non-empty STRING".into(),
            },
            |c| Value::Integer(i128::from(u32::from(c)), IntegerType::Int32),
        ));
    }
    if name == "CHAR" {
        let (code, _) = integer(&arguments[0], span)?;
        return Ok(u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .map_or_else(
                || Value::Error {
                    code: 1,
                    message: "CHAR code is not a Unicode scalar".into(),
                },
                |c| Value::String(c.into()),
            ));
    }
    let math_name = name
        .strip_prefix("BNMath.")
        .ok_or_else(|| runtime_error("NAME_NOT_FOUND", "unknown builtin", span))?;
    if matches!(math_name, "TOHOUR" | "TOWEEKDAY") {
        let milliseconds = integer(&arguments[0], span)?.0;
        let days = milliseconds.div_euclid(86_400_000);
        let result = if math_name == "TOHOUR" {
            milliseconds.div_euclid(3_600_000).rem_euclid(24)
        } else {
            // 1970-01-01 was Thursday (ISO weekday 4).
            (days + 3).rem_euclid(7) + 1
        };
        return Ok(Value::Integer(result, IntegerType::Int32));
    }
    if math_name == "VAL" {
        let Value::String(text) = &arguments[0] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "BNMath.VAL expects STRING",
                span,
            ));
        };
        return Ok(Value::Float(parse_val(text), FloatType::Float64));
    }
    if matches!(
        math_name,
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "MODE" | "STDEV" | "VARIANCE" | "RANGE"
    ) || (matches!(math_name, "MIN" | "MAX") && arguments.len() == 1)
    {
        return reduce_vector(math_name, &arguments[0], span, memory);
    }
    if matches!(math_name, "ABS" | "MIN" | "MAX" | "SIGN")
        && arguments
            .iter()
            .all(|argument| matches!(argument, Value::Integer(_, _)))
    {
        let integers = arguments
            .iter()
            .map(|argument| integer(argument, span).map(|(value, _)| value))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = integer(&arguments[0], span)?.1;
        let result = match math_name {
            "ABS" => integers[0]
                .checked_abs()
                .ok_or_else(|| runtime_error("NUMERIC_OVERFLOW", "BNMath.ABS overflowed", span))?,
            "MIN" => integers[0].min(integers[1]),
            "MAX" => integers[0].max(integers[1]),
            "SIGN" => integers[0].signum(),
            _ => unreachable!(),
        };
        return Ok(Value::Integer(result, kind));
    }
    let numbers = arguments
        .iter()
        .map(|value| number_as_float(value, span))
        .collect::<Result<Vec<_>, _>>()?;
    let result = match math_name {
        "ABS" => numbers[0].abs(),
        "MIN" => {
            if numbers.iter().any(|value| value.is_nan()) {
                f64::NAN
            } else {
                numbers[0].min(numbers[1])
            }
        }
        "MAX" => {
            if numbers.iter().any(|value| value.is_nan()) {
                f64::NAN
            } else {
                numbers[0].max(numbers[1])
            }
        }
        "SIGN" => {
            if numbers[0] == 0.0 || numbers[0].is_nan() {
                numbers[0]
            } else {
                numbers[0].signum()
            }
        }
        "FLOOR" => numbers[0].floor(),
        "CEIL" => numbers[0].ceil(),
        "TRUNC" => numbers[0].trunc(),
        "ROUND" => {
            let scale = 10_f64.powf(numbers[1]);
            (numbers[0] * scale).round_ties_even() / scale
        }
        "EXP" => numbers[0].exp(),
        "LOG" => numbers[0].ln(),
        "LOG10" => numbers[0].log10(),
        "LOG2" => numbers[0].log2(),
        "POW" => numbers[0].powf(numbers[1]),
        "SIN" => numbers[0].sin(),
        "COS" => numbers[0].cos(),
        "TAN" => numbers[0].tan(),
        "ASIN" => numbers[0].asin(),
        "ACOS" => numbers[0].acos(),
        "ATAN" => numbers[0].atan(),
        "ATAN2" => numbers[0].atan2(numbers[1]),
        "SQRT" => numbers[0].sqrt(),
        "HYPOT" => numbers[0].hypot(numbers[1]),
        "FMA" => numbers[0].mul_add(numbers[1], numbers[2]),
        _ => {
            return Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("unknown BNMath function '{math_name}'"),
                span,
            ));
        }
    };
    Ok(Value::Float(result, FloatType::Float64))
}

#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::match_same_arms
)] // Object and pointer identity both compare handles.
fn equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Integer(left, _), Value::Integer(right, _)) => left == right,
        (Value::Float(left, _), Value::Float(right, _)) => left == right,
        (Value::Integer(left, _), Value::Float(right, _)) => *left as f64 == *right,
        (Value::Float(left, _), Value::Integer(right, _)) => *left == *right as f64,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Null, Value::Null)
        | (Value::NotAvailable, Value::NotAvailable)
        | (Value::EndOfFile, Value::EndOfFile)
        | (Value::HostConsole, Value::HostConsole) => true,
        (
            Value::Handle {
                type_name: left, ..
            },
            Value::Handle {
                type_name: right, ..
            },
        ) => left == right,
        (Value::File(left), Value::File(right)) => left == right,
        (Value::DataFrame(left), Value::DataFrame(right)) => left == right,
        (
            Value::Error {
                code: left_code,
                message: left_message,
            },
            Value::Error {
                code: right_code,
                message: right_message,
            },
        ) => left_code == right_code && left_message == right_message,
        (Value::Object { handle: left, .. }, Value::Object { handle: right, .. }) => left == right,
        (Value::Pointer { handle: left }, Value::Pointer { handle: right }) => left == right,
        (Value::Date(left), Value::Date(right)) => left == right,
        (Value::Time(left), Value::Time(right)) => left == right,
        (Value::TimeZone(left), Value::TimeZone(right)) => left == right,
        (
            Value::Record {
                type_name: left_name,
                fields: left,
            },
            Value::Record {
                type_name: right_name,
                fields: right,
            },
        ) => {
            left_name == right_name
                && left.len() == right.len()
                && left
                    .iter()
                    .all(|(name, value)| right.get(name).is_some_and(|other| equals(value, other)))
        }
        (Value::Vector(left), Value::Vector(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equals(left, right))
        }
        _ => false,
    }
}

fn is_value(value: &Value, test: &str) -> bool {
    match test {
        "NAN" => matches!(value, Value::Float(value, _) if value.is_nan()),
        "INF" => matches!(value, Value::Float(value, _) if *value == f64::INFINITY),
        "-INF" => matches!(value, Value::Float(value, _) if *value == f64::NEG_INFINITY),
        "NULL" => matches!(value, Value::Null),
        "NA" => matches!(value, Value::NotAvailable),
        "EOF" => matches!(value, Value::EndOfFile),
        "Error" => matches!(value, Value::Error { .. }),
        "DATE" => matches!(value, Value::Date(_)),
        "TIME" => matches!(value, Value::Time(_)),
        "TIMEZONE" => matches!(value, Value::TimeZone(_)),
        test if test.starts_with("POINTER TO ") => matches!(value, Value::Pointer { .. }),
        _ => match value {
            Value::File(_) => is_host_file_type(test),
            Value::DataFrame(_) => {
                test == "DataFrame" || (test.ends_with(".DataFrame") && !test.starts_with('#'))
            }
            Value::Object { class, .. } => class == test || class.rsplit('.').next() == Some(test),
            Value::Record { type_name, .. } => type_name == test,
            _ => false,
        },
    }
}

fn value_matches_type(value: &Value, ty: &Type) -> bool {
    match (value, ty) {
        (Value::Integer(_, _), Type::Integer(_))
        | (Value::Float(_, _), Type::Float(_))
        | (Value::Boolean(_), Type::Boolean)
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile)
        | (Value::Object { .. }, Type::Named(_) | Type::ImportedNamed { .. })
        | (Value::Pointer { .. }, Type::Pointer { .. }) => true,
        (Value::Date(_), Type::Named(name)) if name == "DATE" => true,
        (Value::Time(_), Type::Named(name)) if name == "TIME" => true,
        (Value::TimeZone(_), Type::Named(name)) if name == "TIMEZONE" => true,
        (Value::Error { .. }, Type::Named(name)) => name == "Error",
        (Value::File(_), Type::Named(name) | Type::TypeName(name)) => is_host_file_type(name),
        (
            Value::DataFrame(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "DataFrame",
        (Value::Null, Type::Named(name)) => name == "VOID",
        (Value::Record { type_name, .. }, Type::Named(name) | Type::ImportedNamed { name, .. }) => {
            type_name == name
        }
        _ => false,
    }
}

fn is_host_file_type(name: &str) -> bool {
    name == "FS.File" || (name.ends_with(".File") && !name.starts_with('#'))
}

fn is_host_file_method(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(owner, method)| {
        is_host_file_type(owner)
            && matches!(
                method,
                "Close"
                    | "ReadLine"
                    | "ReadAll"
                    | "Write"
                    | "ReadBytes"
                    | "WriteBytes"
                    | "WriteLine"
            )
    })
}

fn dataframe_index_error() -> Value {
    Value::Error {
        code: 1,
        message: "DataFrame index out of bounds".into(),
    }
}

fn unsigned_indices(values: Vec<i128>) -> Option<Vec<usize>> {
    values
        .into_iter()
        .map(|value| usize::try_from(value).ok())
        .collect()
}

#[allow(clippy::cast_precision_loss)] // Integer cells become FLOAT before BNMath reductions.
fn dataframe_numeric_values(column: &DataFrameColumn) -> Result<Vec<Value>, &'static str> {
    let mut values = Vec::new();
    for value in &column.values {
        match value {
            Value::Integer(number, _) => {
                values.push(Value::Float(*number as f64, FloatType::Float64));
            }
            Value::Float(number, _) => values.push(Value::Float(*number, FloatType::Float64)),
            Value::NotAvailable => {}
            _ => return Err("column is not numeric"),
        }
    }
    Ok(values)
}

fn collect_indices(
    value: &Value,
    memory: &Heap<Value>,
    span: Span,
) -> Result<Vec<i128>, Diagnostic> {
    let values = match value {
        Value::Vector(values) => values.clone(),
        Value::Pointer { handle } => (0..memory.len(*handle, span)?)
            .map(|index| memory.get(*handle, index, span).cloned())
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "indices must be an INTEGER vector",
                span,
            ));
        }
    };
    values
        .into_iter()
        .map(|value| integer(&value, span).map(|(value, _)| value))
        .collect()
}

fn integer(value: &Value, span: Span) -> Result<(i128, IntegerType), Diagnostic> {
    let Value::Integer(value, kind) = value else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "expected integral value",
            span,
        ));
    };
    Ok((*value, *kind))
}

fn boolean(value: &Value, span: Span) -> Result<bool, Diagnostic> {
    let Value::Boolean(value) = value else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "expected BOOLEAN value",
            span,
        ));
    };
    Ok(*value)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // IEEE conversion is part of BN numeric semantics.
fn number_as_float(value: &Value, span: Span) -> Result<f64, Diagnostic> {
    match value {
        Value::Integer(value, _) => Ok(*value as f64),
        Value::Float(value, kind) => Ok(match kind {
            FloatType::Float32 => f64::from(*value as f32),
            FloatType::Float64 => *value,
        }),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "expected numeric value",
            span,
        )),
    }
}

fn parse_val(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut end = 0;
    if bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')) {
        end = 1;
    }
    let digits = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits || (end == digits + 1 && bytes.get(digits) == Some(&b'.')) {
        return 0.0;
    }
    text[..end].parse().unwrap_or(0.0)
}

#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::too_many_lines
)]
fn reduce_vector(
    name: &str,
    value: &Value,
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    let owned;
    let values = match value {
        Value::Vector(values) => values,
        Value::Pointer { handle } => {
            let len = memory.len(*handle, span)?;
            owned = (0..len)
                .map(|index| memory.get(*handle, index, span).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            &owned
        }
        _ => {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "BNMath reduction expects a vector",
                span,
            ));
        }
    };
    let mut numbers = values
        .iter()
        .map(|v| number_as_float(v, span))
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(name, "MIN" | "MAX") {
        if numbers.is_empty() {
            return Err(runtime_error(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
                span,
            ));
        }
        let first = values.first().ok_or_else(|| {
            runtime_error(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
                span,
            )
        })?;
        if let Value::Integer(_, kind) = first {
            let integers = values
                .iter()
                .map(|value| integer(value, span).map(|(value, _)| value))
                .collect::<Result<Vec<_>, _>>()?;
            let result = if name == "MIN" {
                integers.iter().copied().reduce(i128::min)
            } else {
                integers.iter().copied().reduce(i128::max)
            }
            .ok_or_else(|| {
                runtime_error(
                    "INDEX_OUT_OF_BOUNDS",
                    "BNMath reduction received an empty vector",
                    span,
                )
            })?;
            return Ok(Value::Integer(result, *kind));
        }
        if numbers.iter().any(|value| value.is_nan()) {
            let Value::Float(_, kind) = first else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNMath reduction expects numeric values",
                    span,
                ));
            };
            return Ok(Value::Float(f64::NAN, *kind));
        }
        let Value::Float(_, kind) = first else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "BNMath reduction expects numeric values",
                span,
            ));
        };
        return Ok(Value::Float(
            if name == "MIN" {
                numbers.iter().copied().fold(f64::INFINITY, f64::min)
            } else {
                numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            },
            *kind,
        ));
    }
    if numbers.iter().any(|v| v.is_nan()) {
        return Ok(Value::Float(f64::NAN, FloatType::Float64));
    }
    if name == "MODE" && numbers.is_empty() {
        return Ok(Value::NotAvailable);
    }
    if numbers.is_empty() || (matches!(name, "STDEV" | "VARIANCE") && numbers.len() < 2) {
        return Ok(Value::Float(f64::NAN, FloatType::Float64));
    }
    if matches!(name, "QUARTILE1" | "QUARTILE3") && numbers.len() < 2 {
        return Ok(Value::Float(f64::NAN, FloatType::Float64));
    }
    numbers.sort_by(f64::total_cmp);
    let median = |xs: &[f64]| {
        if xs.len() % 2 == 1 {
            xs[xs.len() / 2]
        } else {
            (xs[xs.len() / 2 - 1] + xs[xs.len() / 2]) / 2.0
        }
    };
    let result = match name {
        "MEAN" => numbers.iter().sum::<f64>() / numbers.len() as f64,
        "MEDIAN" => median(&numbers),
        "QUARTILE1" => median(&numbers[..numbers.len() / 2]),
        "QUARTILE3" => median(&numbers[numbers.len().div_ceil(2)..]),
        "RANGE" => numbers[numbers.len() - 1] - numbers[0],
        "VARIANCE" | "STDEV" => {
            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            let variance = numbers.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                / (numbers.len() - 1) as f64;
            if name == "STDEV" {
                variance.sqrt()
            } else {
                variance
            }
        }
        "MODE" => {
            let mut best = None;
            let mut count = 0;
            let mut tie = false;
            for &v in &numbers {
                let c = numbers.iter().filter(|x| **x == v).count();
                if c > count {
                    best = Some(v);
                    count = c;
                    tie = false;
                } else if c == count && best != Some(v) {
                    tie = true;
                }
            }
            if tie {
                return Ok(Value::NotAvailable);
            }
            best.unwrap_or(f64::NAN)
        }
        _ => unreachable!(),
    };
    Ok(Value::Float(result, FloatType::Float64))
}

fn is_float_value(value: &Value) -> bool {
    matches!(value, Value::Float(_, _))
}

fn integer_kind(ty: &Type) -> Option<IntegerType> {
    match ty {
        Type::Integer(kind) => Some(*kind),
        _ => None,
    }
}

fn float_kind(ty: &Type) -> FloatType {
    match ty {
        Type::Float(kind) => *kind,
        _ => FloatType::Float64,
    }
}

#[allow(clippy::cast_possible_truncation)] // FLOAT32 storage requires IEEE binary32 rounding.
fn float_value(value: f64, kind: FloatType) -> Value {
    Value::Float(
        match kind {
            FloatType::Float32 => f64::from(value as f32),
            FloatType::Float64 => value,
        },
        kind,
    )
}

fn integer_width(kind: IntegerType) -> u8 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 8,
        IntegerType::Int16 | IntegerType::UInt16 => 16,
        IntegerType::Int32 | IntegerType::UInt32 => 32,
        IntegerType::Int64 | IntegerType::UInt64 => 64,
    }
}

fn integer_range(kind: IntegerType) -> (i128, i128) {
    match kind {
        IntegerType::Byte => (0, u8::MAX.into()),
        IntegerType::Int8 => (i8::MIN.into(), i8::MAX.into()),
        IntegerType::Int16 => (i16::MIN.into(), i16::MAX.into()),
        IntegerType::Int32 => (i32::MIN.into(), i32::MAX.into()),
        IntegerType::Int64 => (i64::MIN.into(), i64::MAX.into()),
        IntegerType::UInt16 => (0, u16::MAX.into()),
        IntegerType::UInt32 => (0, u32::MAX.into()),
        IntegerType::UInt64 => (0, u64::MAX.into()),
    }
}

fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn parse_float(value: &str) -> f64 {
    match value {
        "NAN" => f64::NAN,
        "INF" => f64::INFINITY,
        "-INF" => f64::NEG_INFINITY,
        _ => value.parse().expect("validated FLOAT literal"),
    }
}

fn render(value: &Value) -> String {
    match value {
        Value::Integer(value, _) => value.to_string(),
        Value::Float(value, _) if value.is_nan() => "NAN".into(),
        Value::Float(value, _) if *value == f64::INFINITY => "INF".into(),
        Value::Float(value, _) if *value == f64::NEG_INFINITY => "-INF".into(),
        Value::Float(value, _) => {
            let mut text = value.to_string();
            if !text.contains(['.', 'e', 'E']) {
                text.push_str(".0");
            }
            text
        }
        Value::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.into(),
        Value::String(value) => value.clone(),
        Value::Null => "NULL".into(),
        Value::NotAvailable => "NA".into(),
        Value::EndOfFile => "EOF".into(),
        Value::Vector(values) => format!(
            "[{}]",
            values.iter().map(render).collect::<Vec<_>>().join(", ")
        ),
        Value::Function(name) | Value::Type(name) | Value::TimeZone(name) => name.clone(),
        Value::HostConsole => "HOST.Console".into(),
        Value::HostArgs => "HOST.Args".into(),
        Value::Handle { type_name } | Value::Record { type_name, .. } => type_name.clone(),
        Value::Object { class, .. } => class.rsplit('.').next().unwrap_or(class).to_string(),
        Value::Pointer { .. } => "POINTER".into(),
        Value::File(_) => "FS.File".into(),
        Value::DataFrame(_) => "DataFrame".into(),
        Value::Date(days) => temporal::format_date(*days),
        Value::Time(millis) => temporal::format_time(*millis),
        Value::Error { code, message } => format!("Error({code}, {message})"),
    }
}

fn exit_code(code: i128, span: Span) -> Result<u8, Diagnostic> {
    u8::try_from(code)
        .map_err(|_| runtime_error("INVALID_EXIT_CODE", "exit code must be in 0..255", span))
}

fn ordered<T: Ord>(operator: &str, left: &T, right: &T, span: Span) -> Result<Value, Diagnostic> {
    match operator {
        "Less" => Ok(Value::Boolean(left < right)),
        "LessEqual" => Ok(Value::Boolean(left <= right)),
        "Greater" => Ok(Value::Boolean(left > right)),
        "GreaterEqual" => Ok(Value::Boolean(left >= right)),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid ordered comparison",
            span,
        )),
    }
}

fn lifecycle_dispatch(name: &str, arguments: &[Value]) -> Option<(Handle, String)> {
    let class = name
        .strip_suffix(".CONSTRUCTOR")
        .or_else(|| name.strip_suffix(".DESTRUCTOR"))
        .or_else(|| name.strip_suffix(".$fields"))?;
    let Value::Object { handle, .. } = arguments.first()? else {
        return None;
    };
    Some((*handle, class.into()))
}

fn require_console(value: &Value, span: Span) -> Result<(), Diagnostic> {
    if matches!(value, Value::HostConsole) {
        Ok(())
    } else {
        Err(runtime_error(
            "TYPE_MISMATCH",
            "CLS and BEEP require HOST.Console",
            span,
        ))
    }
}

fn integer_from_count(count: usize, span: Span) -> Result<Value, Diagnostic> {
    let count = i128::try_from(count).map_err(|_| integer_overflow(span))?;
    integer_from_i128_count(count, span)
}

fn integer_from_i128_count(count: i128, span: Span) -> Result<Value, Diagnostic> {
    if !(0..=i128::from(i32::MAX)).contains(&count) {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(count, IntegerType::Int32))
}

fn integer_from_u64(count: u64, span: Span) -> Result<Value, Diagnostic> {
    if count > 2_147_483_647 {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(i128::from(count), IntegerType::Int32))
}

fn integer_overflow(span: Span) -> Diagnostic {
    runtime_error("NUMERIC_OVERFLOW", "result does not fit INTEGER", span)
}

fn runtime_error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}

fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
