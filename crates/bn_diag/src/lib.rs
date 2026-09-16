// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stable diagnostic facts shared by frontend, IR and backend crates.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use bn_source::{Position, Revision, SourceFile, SourceId, Span};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagId {
    Lexical,
    Parse,
    TypeMismatch,
    NumericOverflow,
    InvalidIr,
    IrLowering,
    ModuleNotFound,
    Bnc,
    BncEngine,
    DoubleRelease,
    FunctionNotFound,
    UseAfterRelease,
    TargetUnsupportedEntrypoint,
    TargetUnsupportedHost,
    TargetUnsupportedOp,
    TargetUnsupportedType,
    TargetUnsupportedLlvm,
    UnusedBinding,
    UnusedImport,
    UnreachableCode,
    /// Runtime and toolchain codes retained as stable legacy identifiers while
    /// their richer argument schemas are migrated incrementally.
    Runtime(&'static str),
}

impl DiagId {
    const LEGACY_MESSAGE_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "message",
        kind: ArgumentKind::Text,
    }];
    const UNUSED_BINDING_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "name",
        kind: ArgumentKind::Text,
    }];
    const LEXICAL_SCHEMA: &'static [ArgumentSpec] = &[
        ArgumentSpec {
            name: "found",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "expected",
            kind: ArgumentKind::Text,
        },
    ];
    const PARSE_SCHEMA: &'static [ArgumentSpec] = &[
        ArgumentSpec {
            name: "expected",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "context",
            kind: ArgumentKind::Text,
        },
    ];
    const DUPLICATE_NAME_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "name",
        kind: ArgumentKind::Text,
    }];
    const NAME_NOT_FOUND_SCHEMA: &'static [ArgumentSpec] = &[
        ArgumentSpec {
            name: "name",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "context",
            kind: ArgumentKind::Text,
        },
    ];
    const NUMERIC_OVERFLOW_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "operation",
        kind: ArgumentKind::Text,
    }];
    const DIVISION_BY_ZERO_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "operation",
        kind: ArgumentKind::Text,
    }];
    const INDEX_OUT_OF_BOUNDS_SCHEMA: &'static [ArgumentSpec] = &[
        ArgumentSpec {
            name: "index",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "bound",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "context",
            kind: ArgumentKind::Text,
        },
    ];
    const TYPE_MISMATCH_SCHEMA: &'static [ArgumentSpec] = &[
        ArgumentSpec {
            name: "expected",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "actual",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "context",
            kind: ArgumentKind::Text,
        },
    ];
    const UNUSED_IMPORT_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "module",
        kind: ArgumentKind::Text,
    }];
    const UNREACHABLE_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "context",
        kind: ArgumentKind::Text,
    }];
    const TARGET_SUPPORT_SCHEMA: &'static [ArgumentSpec] = &[
        ArgumentSpec {
            name: "target",
            kind: ArgumentKind::Text,
        },
        ArgumentSpec {
            name: "detail",
            kind: ArgumentKind::Text,
        },
    ];
    const DETAIL_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "detail",
        kind: ArgumentKind::Text,
    }];
    const PATH_SCHEMA: &'static [ArgumentSpec] = &[ArgumentSpec {
        name: "path",
        kind: ArgumentKind::Text,
    }];
    const RUNTIME_CODES: &'static [&'static str] = &[
        "ALLOCATION_SIZE_INVALID",
        "ALLOCATION_SIZE_OVERFLOW",
        "ALLOCATION_TOO_LARGE",
        "ASYNC_RETURN_TYPE",
        "ASYNC_TARGET",
        "AWAIT_TIMEOUT",
        "BUILD_EMISSION_FAILED",
        "BUILD_TOOLCHAIN_UNAVAILABLE",
        "CONFIG_INVALID",
        "DEBUG_TERMINATED",
        "DISPATCH",
        "DIVISION_BY_ZERO",
        "DUPLICATE_INTERFACE",
        "DUPLICATE_NAME",
        "EVAL_START_PROMOTED",
        "DOUBLE_DELETE",
        "EXECUTION_POLICY_DENIED",
        "FORMAT_OUT_OF_RANGE",
        "HANDLER_NOT_FOUND",
        "HEADER_NOT_FOUND",
        "HOST_CAPABILITY_UNAVAILABLE",
        "HOST_ARGS_SCOPE",
        "HOST_IMPORT_SCOPE",
        "IMPORTED_START",
        "IMPORT_CYCLE",
        "INDEX_OUT_OF_BOUNDS",
        "INPUT_ERROR",
        "INPUT_PROMPT_TYPE",
        "INHERITANCE_CYCLE",
        "INVALID_ALTERNATIVE_USE",
        "INVALID_CONSTRUCTOR",
        "INVALID_DESTRUCTOR",
        "INVALID_EGRESS_POLICY",
        "INVALID_DATE",
        "INVALID_EXIT_CODE",
        "INVALID_EXPONENT",
        "INVALID_FILE_MODE",
        "INVALID_FOR_STEP",
        "INVALID_HOST_ARGS_USE",
        "INVALID_INPUT",
        "INVALID_JSON",
        "INVALID_LOOP_CONTROL",
        "INVALID_NUMERIC_CONVERSION",
        "INVALID_OPTIONS",
        "INVALID_OVERRIDE",
        "INVALID_POINTER_TYPE",
        "INVALID_RELEASE_TARGET",
        "INVALID_SHIFT_COUNT",
        "INVALID_START",
        "INVALID_SUPER",
        "INVALID_TIME",
        "INVALID_TIMEZONE",
        "INVALID_VALUE",
        "INVALID_VECTOR_DIMENSION",
        "INVALID_VECTOR_TYPE",
        "IO",
        "LIMIT",
        "MISSING_RETURN",
        "MODULE_NOT_RESOLVED",
        "MODULE_LIMIT",
        "NAME_NOT_FOUND",
        "NOT_FOUND",
        "NOT_CALLABLE",
        "NULL_POINTER_ACCESS",
        "OUTPUT_ERROR",
        "PARSE_ERROR",
        "POINTER_LENGTH_MISMATCH",
        "PRIVATE_ACCESS",
        "PROCESS_LOG_WRITE",
        "REQUEST_INVALID",
        "RETAIN_OVERFLOW",
        "RESOURCE_LIMIT",
        "SCRAPER_INPUT",
        "SERVER_STATE",
        "SESSION_CONFIG",
        "STALE_HANDLE",
        "START_NOT_FOUND",
        "STATIC_INITIALIZATION_CYCLE",
        "TLS_PROVIDER_UNAVAILABLE",
        "TYPE_NAME_AS_VALUE",
        "UNINITIALIZED_VALUE",
        "UNKNOWN_TYPE",
        "UNRESOLVED_TYPE",
        "USE_AFTER_DELETE",
        "VECTOR_LENGTH_MISMATCH",
        "WEB_LISTEN",
    ];

    #[must_use]
    pub const fn fluent_id(self) -> &'static str {
        match self {
            Self::Lexical => "lexical-error",
            Self::Parse => "parse-error",
            Self::TypeMismatch => "type-mismatch",
            Self::NumericOverflow => "numeric-overflow",
            Self::InvalidIr => "invalid-ir",
            Self::IrLowering => "ir-lowering",
            Self::ModuleNotFound => "module-not-found",
            Self::Bnc => "bnc",
            Self::BncEngine => "bnc-engine",
            Self::DoubleRelease => "double-release",
            Self::FunctionNotFound => "function-not-found",
            Self::UseAfterRelease => "use-after-release",
            Self::TargetUnsupportedEntrypoint => "target-unsupported-entrypoint",
            Self::TargetUnsupportedHost => "target-unsupported-host",
            Self::TargetUnsupportedOp => "target-unsupported-op",
            Self::TargetUnsupportedType => "target-unsupported-type",
            Self::TargetUnsupportedLlvm => "target-unsupported-llvm",
            Self::UnusedBinding => "unused-binding",
            Self::UnusedImport => "unused-import",
            Self::UnreachableCode => "unreachable-code",
            Self::Runtime(code) => code,
        }
    }

    #[must_use]
    pub fn from_fluent_id(id: &str) -> Option<Self> {
        [
            Self::Lexical,
            Self::Parse,
            Self::TypeMismatch,
            Self::NumericOverflow,
            Self::InvalidIr,
            Self::IrLowering,
            Self::ModuleNotFound,
            Self::Bnc,
            Self::BncEngine,
            Self::DoubleRelease,
            Self::FunctionNotFound,
            Self::UseAfterRelease,
            Self::TargetUnsupportedEntrypoint,
            Self::TargetUnsupportedHost,
            Self::TargetUnsupportedOp,
            Self::TargetUnsupportedType,
            Self::TargetUnsupportedLlvm,
            Self::UnusedBinding,
            Self::UnusedImport,
            Self::UnreachableCode,
        ]
        .into_iter()
        .find(|candidate| candidate.fluent_id() == id)
        .or_else(|| Self::from_code(id))
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Lexical => "E0001",
            Self::Parse => "E0100",
            Self::TypeMismatch => "TYPE_MISMATCH",
            Self::NumericOverflow => "NUMERIC_OVERFLOW",
            Self::InvalidIr => "INVALID_IR",
            Self::IrLowering => "IR_LOWERING",
            Self::ModuleNotFound => "MODULE_NOT_FOUND",
            Self::Bnc => "BNC",
            Self::BncEngine => "BNC_ENGINE",
            Self::DoubleRelease => "DOUBLE_RELEASE",
            Self::FunctionNotFound => "FUNCTION_NOT_FOUND",
            Self::UseAfterRelease => "USE_AFTER_RELEASE",
            Self::TargetUnsupportedEntrypoint => "TARGET_UNSUPPORTED_ENTRYPOINT",
            Self::TargetUnsupportedHost => "TARGET_UNSUPPORTED_HOST",
            Self::TargetUnsupportedOp => "TARGET_UNSUPPORTED_OP",
            Self::TargetUnsupportedType => "TARGET_UNSUPPORTED_TYPE",
            Self::TargetUnsupportedLlvm => "TARGET_UNSUPPORTED_LLVM",
            Self::UnusedBinding => "UNUSED_BINDING",
            Self::UnusedImport => "UNUSED_IMPORT",
            Self::UnreachableCode => "UNREACHABLE_CODE",
            Self::Runtime(code) => code,
        }
    }

    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        [
            Self::Lexical,
            Self::Parse,
            Self::TypeMismatch,
            Self::NumericOverflow,
            Self::InvalidIr,
            Self::IrLowering,
            Self::ModuleNotFound,
            Self::Bnc,
            Self::BncEngine,
            Self::DoubleRelease,
            Self::FunctionNotFound,
            Self::UseAfterRelease,
            Self::TargetUnsupportedEntrypoint,
            Self::TargetUnsupportedHost,
            Self::TargetUnsupportedOp,
            Self::TargetUnsupportedType,
            Self::TargetUnsupportedLlvm,
            Self::UnusedBinding,
            Self::UnusedImport,
            Self::UnreachableCode,
        ]
        .into_iter()
        .find(|id| id.code() == code)
        .or_else(|| {
            Self::RUNTIME_CODES
                .iter()
                .copied()
                .find(|candidate| *candidate == code)
                .map(Self::Runtime)
        })
    }

    fn all() -> impl Iterator<Item = Self> {
        [
            Self::Lexical,
            Self::Parse,
            Self::TypeMismatch,
            Self::NumericOverflow,
            Self::InvalidIr,
            Self::IrLowering,
            Self::ModuleNotFound,
            Self::Bnc,
            Self::BncEngine,
            Self::DoubleRelease,
            Self::FunctionNotFound,
            Self::UseAfterRelease,
            Self::TargetUnsupportedEntrypoint,
            Self::TargetUnsupportedHost,
            Self::TargetUnsupportedOp,
            Self::TargetUnsupportedType,
            Self::TargetUnsupportedLlvm,
            Self::UnusedBinding,
            Self::UnusedImport,
            Self::UnreachableCode,
        ]
        .into_iter()
        .chain(Self::RUNTIME_CODES.iter().copied().map(Self::Runtime))
    }

    #[must_use]
    pub fn severity(self) -> Severity {
        match self {
            Self::UnusedBinding
            | Self::UnusedImport
            | Self::UnreachableCode
            | Self::Runtime("EVAL_START_PROMOTED") => Severity::Warning,
            _ => Severity::Error,
        }
    }

    #[must_use]
    pub fn warnings_allowed(self) -> bool {
        matches!(self.severity(), Severity::Warning)
    }

    /// Typed argument contract for this identity. DX03 replaces the legacy
    /// message-only schemas as each producer is migrated.
    #[must_use]
    #[allow(clippy::match_same_arms, clippy::match_wildcard_for_single_variants)]
    pub fn argument_schema(self) -> &'static [ArgumentSpec] {
        match self {
            Self::Lexical => Self::LEXICAL_SCHEMA,
            Self::Parse => Self::PARSE_SCHEMA,
            Self::Runtime("DUPLICATE_NAME") => Self::DUPLICATE_NAME_SCHEMA,
            Self::Runtime("NAME_NOT_FOUND") => Self::NAME_NOT_FOUND_SCHEMA,
            Self::Runtime("DIVISION_BY_ZERO") => Self::DIVISION_BY_ZERO_SCHEMA,
            Self::Runtime("INDEX_OUT_OF_BOUNDS") => Self::INDEX_OUT_OF_BOUNDS_SCHEMA,
            Self::TypeMismatch => Self::TYPE_MISMATCH_SCHEMA,
            Self::NumericOverflow => Self::NUMERIC_OVERFLOW_SCHEMA,
            Self::UnusedBinding => Self::UNUSED_BINDING_SCHEMA,
            Self::UnusedImport => Self::UNUSED_IMPORT_SCHEMA,
            Self::UnreachableCode => Self::UNREACHABLE_SCHEMA,
            Self::TargetUnsupportedEntrypoint
            | Self::TargetUnsupportedHost
            | Self::TargetUnsupportedOp
            | Self::TargetUnsupportedType
            | Self::TargetUnsupportedLlvm => Self::TARGET_SUPPORT_SCHEMA,
            Self::IrLowering | Self::InvalidIr | Self::Runtime("IMPORT_CYCLE" | "MODULE_LIMIT") => {
                Self::DETAIL_SCHEMA
            }
            Self::ModuleNotFound => Self::PATH_SCHEMA,
            Self::Runtime(
                "ALLOCATION_TOO_LARGE"
                | "INVALID_DATE"
                | "INVALID_TIME"
                | "INVALID_TIMEZONE"
                | "ALLOCATION_SIZE_INVALID"
                | "ALLOCATION_SIZE_OVERFLOW",
            ) => Self::DETAIL_SCHEMA,
            Self::DoubleRelease | Self::UseAfterRelease => Self::DETAIL_SCHEMA,
            Self::FunctionNotFound | Self::Runtime("INVALID_START") => Self::DETAIL_SCHEMA,
            Self::Runtime("HOST_CAPABILITY_UNAVAILABLE" | "EXECUTION_POLICY_DENIED") => {
                Self::DETAIL_SCHEMA
            }
            Self::Runtime("INVALID_EXIT_CODE" | "INVALID_EXPONENT" | "INVALID_SHIFT_COUNT") => {
                Self::DETAIL_SCHEMA
            }
            Self::Runtime("INVALID_VALUE" | "INVALID_INPUT" | "INPUT_ERROR") => Self::DETAIL_SCHEMA,
            Self::Runtime("DISPATCH" | "INVALID_JSON" | "INVALID_EGRESS_POLICY") => {
                Self::DETAIL_SCHEMA
            }
            Self::Bnc | Self::BncEngine => Self::DETAIL_SCHEMA,
            _ => Self::LEGACY_MESSAGE_SCHEMA,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Allow,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelStyle {
    Primary,
    Secondary,
}

#[derive(Clone, Debug, Default)]
pub struct WarningPolicy {
    config_default: Option<Level>,
    config_levels: HashMap<DiagId, Level>,
    cli_levels: HashMap<DiagId, Level>,
    warnings_as_errors: bool,
}

impl WarningPolicy {
    /// Parses the warning subset defined by the 0.4.5 `config.toml` contract.
    /// Unknown keys outside the warning tables are ignored by design.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed entries, unknown diagnostic codes,
    /// invalid levels or an invalid global default.
    pub fn from_config(text: &str) -> Result<Self, String> {
        let mut policy = Self::default();
        let mut section = "";
        let mut seen = HashSet::new();
        for raw in text.lines() {
            let line = strip_config_comment(raw)?.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("malformed warning configuration: {line}"));
            };
            let key = key.trim();
            let full_key = format!("{section}.{}", key.trim());
            if matches!(section, "warnings" | "warnings.levels") && !seen.insert(full_key) {
                return Err(format!(
                    "duplicate warning configuration key: {}",
                    key.trim()
                ));
            }
            match section {
                "warnings" if key == "default" => {
                    let value = parse_config_string(value.trim())?;
                    let level = parse_level(&value)?;
                    if level == Level::Allow {
                        return Err("warning default cannot be allow".into());
                    }
                    policy.config_default = Some(level);
                }
                "warnings.levels" => {
                    let value = parse_config_string(value.trim())?;
                    let id = DiagId::from_code(key)
                        .ok_or_else(|| format!("unknown diagnostic code in warnings: {key}"))?;
                    policy.set_config(id, parse_level(&value)?)?;
                }
                "warnings" => return Err(format!("unknown warnings key: {key}")),
                _ => {}
            }
        }
        Ok(policy)
    }

    /// # Errors
    ///
    /// Returns an error when `allow` is requested for a hard diagnostic.
    pub fn set_config(&mut self, id: DiagId, level: Level) -> Result<(), String> {
        validate_level(id, level)?;
        self.config_levels.insert(id, level);
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error when `allow` is requested for a hard diagnostic.
    pub fn set_cli(&mut self, id: DiagId, level: Level) -> Result<(), String> {
        validate_level(id, level)?;
        self.cli_levels.insert(id, level);
        Ok(())
    }

    pub fn set_warnings_as_errors(&mut self, enabled: bool) {
        self.warnings_as_errors = enabled;
    }

    #[must_use]
    pub fn level(&self, id: DiagId) -> Level {
        if !id.warnings_allowed() {
            return Level::Error;
        }
        if let Some(level) = self.cli_levels.get(&id) {
            return *level;
        }
        if self.warnings_as_errors {
            return Level::Error;
        }
        if let Some(level) = self.config_levels.get(&id) {
            return *level;
        }
        if let Some(level) = self.config_default {
            return level;
        }
        Level::Warn
    }
}

fn strip_config_comment(line: &str) -> Result<&str, String> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return Ok(&line[..index]),
            _ => {}
        }
    }
    if quoted || escaped {
        return Err("unterminated quoted config value".into());
    }
    Ok(line)
}

fn parse_config_string(value: &str) -> Result<String, String> {
    if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
        return Err(format!(
            "configuration value must be a quoted string: {value}"
        ));
    }
    let mut output = String::new();
    let mut characters = value[1..value.len() - 1].chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        output.push(match characters.next() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('n') => '\n',
            Some('t') => '\t',
            Some(other) => return Err(format!("unsupported config escape: \\{other}")),
            None => return Err("unterminated config escape".into()),
        });
    }
    Ok(output)
}

fn parse_level(value: &str) -> Result<Level, String> {
    match value {
        "allow" => Ok(Level::Allow),
        "warn" => Ok(Level::Warn),
        "error" => Ok(Level::Error),
        _ => Err(format!("invalid warning level: {value}")),
    }
}

fn validate_level(id: DiagId, level: Level) -> Result<(), String> {
    if level == Level::Allow && !id.warnings_allowed() {
        return Err(format!("cannot allow hard diagnostic {}", id.code()));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Label {
    pub span: Span,
    pub style: LabelStyle,
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentKind {
    Text,
    Signed,
    Unsigned,
    Boolean,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArgumentSpec {
    pub name: &'static str,
    pub kind: ArgumentKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticValue {
    Text(String),
    Signed(i64),
    Unsigned(u64),
    Boolean(bool),
}

impl fmt::Display for DiagnosticValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) => formatter.write_str(value),
            Self::Signed(value) => value.fmt(formatter),
            Self::Unsigned(value) => value.fmt(formatter),
            Self::Boolean(value) => value.fmt(formatter),
        }
    }
}

impl DiagnosticValue {
    #[must_use]
    pub const fn kind(&self) -> ArgumentKind {
        match self {
            Self::Text(_) => ArgumentKind::Text,
            Self::Signed(_) => ArgumentKind::Signed,
            Self::Unsigned(_) => ArgumentKind::Unsigned,
            Self::Boolean(_) => ArgumentKind::Boolean,
        }
    }
}

impl From<String> for DiagnosticValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for DiagnosticValue {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

impl From<i64> for DiagnosticValue {
    fn from(value: i64) -> Self {
        Self::Signed(value)
    }
}

impl From<u64> for DiagnosticValue {
    fn from(value: u64) -> Self {
        Self::Unsigned(value)
    }
}

impl From<bool> for DiagnosticValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticSpec {
    pub id: DiagId,
    /// Severity after warning policy, retained as a fact rather than encoded in prose.
    pub effective_severity: Severity,
    pub args: Vec<(String, DiagnosticValue)>,
    pub labels: Vec<Label>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogEntry {
    pub id: DiagId,
    pub code: &'static str,
    pub title: String,
    pub message: String,
    pub label: Option<String>,
    pub label_secondary: Option<String>,
    pub causes: Vec<String>,
    pub help: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub title: String,
    pub message: String,
    pub labels: Vec<Label>,
    pub causes: Vec<String>,
    pub help: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    entries: HashMap<DiagId, CatalogEntry>,
}

impl Catalog {
    /// Selects one immutable catalog using environment, configured directory,
    /// installation share directory, then embedded en-US precedence.
    ///
    /// # Errors
    ///
    /// Returns an error when the selected overlay cannot be read or validated.
    pub fn selected(configured_directory: Option<&Path>) -> Result<Self, String> {
        let environment = std::env::var_os("BN_DIAGNOSTICS_DIR").map(std::path::PathBuf::from);
        let beside = std::env::current_exe().ok().and_then(|exe| {
            let parent = exe.parent()?;
            let local = parent.join("share/bn/diagnostics/en-US");
            if local.is_dir() {
                return Some(local);
            }
            let prefix = parent.parent()?.join("share/bn/diagnostics/en-US");
            prefix.is_dir().then_some(prefix)
        });
        Self::selected_from_paths(
            environment.as_deref(),
            configured_directory,
            beside.as_deref(),
        )
    }

    /// Pure selection primitive used to verify path precedence without
    /// mutating process environment.
    ///
    /// # Errors
    ///
    /// Returns an error when the selected overlay cannot be read or validated.
    pub fn selected_from_paths(
        environment: Option<&Path>,
        configured: Option<&Path>,
        installed: Option<&Path>,
    ) -> Result<Self, String> {
        if let Some(directory) = environment.or(configured).or(installed) {
            return Self::embedded_en_us_with_overlay(directory);
        }
        Self::embedded_en_us()
    }

    /// Returns the process-wide validated embedded `en-US` catalog.
    ///
    /// # Panics
    ///
    /// Panics only if the repository's embedded catalog is invalid; that is a
    /// programmer/build-time invariant.
    #[must_use]
    pub fn embedded_global() -> &'static Self {
        static CATALOG: OnceLock<Catalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            Self::embedded_en_us().expect("embedded en-US diagnostics must be valid")
        })
    }

    /// Returns the process-wide catalog selected by the CLI environment.
    ///
    /// `BN_DIAGNOSTICS_DIR` is intentionally resolved once, so all diagnostics
    /// in one process observe the same immutable catalog. Without the
    /// variable, the validated embedded catalog is used.
    ///
    /// # Errors
    ///
    /// Returns an invalid overlay error without panicking.
    pub fn global_for_environment() -> Result<&'static Self, String> {
        static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
        match CATALOG.get_or_init(|| Self::selected(None)) {
            Ok(catalog) => Ok(catalog),
            Err(error) => Err(error.clone()),
        }
    }

    /// Loads and validates the embedded `en-US` Fluent shards.
    ///
    /// # Errors
    ///
    /// Returns a catalog error if a shard has an unknown or duplicate id, a
    /// duplicate display code, a malformed attribute, or misses a registry id.
    pub fn embedded_en_us() -> Result<Self, String> {
        Self::from_shards([
            include_str!("../../../share/bn/diagnostics/en-US/lex.ftl"),
            include_str!("../../../share/bn/diagnostics/en-US/parse.ftl"),
            include_str!("../../../share/bn/diagnostics/en-US/sem.ftl"),
            include_str!("../../../share/bn/diagnostics/en-US/runtime.ftl"),
            include_str!("../../../share/bn/diagnostics/en-US/build.ftl"),
            include_str!("../../../share/bn/diagnostics/en-US/lsp.ftl"),
        ])
    }

    /// Loads embedded `en-US` and replaces entries found in an overlay
    /// directory. Missing overlay shards and entries fall back to embedded
    /// messages.
    ///
    /// # Errors
    ///
    /// Returns an error when an overlay shard cannot be read, contains a
    /// duplicate entry, or contains invalid Fluent attributes.
    pub fn embedded_en_us_with_overlay(directory: &Path) -> Result<Self, String> {
        if !directory.is_dir() {
            return Err(format!(
                "diagnostic overlay is not a readable directory: {}",
                directory.display()
            ));
        }
        let mut catalog = Self::embedded_en_us()?;
        let mut overlay_ids = HashSet::new();
        for shard in ["lex", "parse", "sem", "runtime", "build", "lsp"] {
            let path = directory.join(format!("{shard}.ftl"));
            if !path.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|error| {
                format!("cannot read diagnostic overlay {}: {error}", path.display())
            })?;
            for entry in parse_shard(&text)? {
                validate_catalog_entry(&entry)?;
                if !overlay_ids.insert(entry.id) {
                    return Err(format!(
                        "duplicate diagnostic id in overlay: {}",
                        entry.id.code()
                    ));
                }
                catalog.entries.insert(entry.id, entry);
            }
        }
        Ok(catalog)
    }

    /// Parses the supported Fluent message subset from one or more shards.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed entries or incomplete registry coverage.
    pub fn from_ftl(text: &str) -> Result<Self, String> {
        Self::from_shards([text])
    }

    fn from_shards<const N: usize>(shards: [&str; N]) -> Result<Self, String> {
        let mut entries = HashMap::new();
        let mut codes = HashSet::new();
        for shard in shards {
            let parsed = parse_shard(shard)?;
            for entry in parsed {
                validate_catalog_entry(&entry)?;
                if entries.insert(entry.id, entry).is_some() {
                    return Err("duplicate diagnostic id".into());
                }
            }
        }
        for entry in entries.values() {
            if !codes.insert(entry.code) {
                return Err("duplicate diagnostic code".into());
            }
        }
        if let Some(missing) = DiagId::all().find(|id| !entries.contains_key(id)) {
            return Err(format!("missing catalog entry for {}", missing.code()));
        }
        Ok(Self { entries })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Formats one spec only after it has survived sink filtering/deduplication.
    ///
    /// # Errors
    ///
    /// Returns an error when the spec has no catalog entry or an argument is
    /// missing from its message pattern.
    pub fn render(&self, spec: &DiagnosticSpec) -> Result<RenderedDiagnostic, String> {
        validate_arguments(spec)?;
        let entry = self
            .entries
            .get(&spec.id)
            .ok_or_else(|| format!("missing catalog entry for {}", spec.id.code()))?;
        let message = substitute(&entry.message, &spec.args)?;
        let causes = entry
            .causes
            .iter()
            .map(|cause| substitute(cause, &spec.args))
            .collect::<Result<Vec<_>, _>>()?;
        let help = entry
            .help
            .as_ref()
            .map(|help| substitute(help, &spec.args))
            .transpose()?;
        let labels = spec
            .labels
            .iter()
            .map(|label| {
                let default = match label.style {
                    LabelStyle::Primary => entry.label.as_ref(),
                    LabelStyle::Secondary => entry.label_secondary.as_ref(),
                };
                let text = match (&label.text, default) {
                    (Some(text), _) => Some(text.clone()),
                    (None, Some(pattern)) => Some(substitute(pattern, &spec.args)?),
                    (None, None) => None,
                };
                Ok(Label {
                    span: label.span,
                    style: label.style,
                    text,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(RenderedDiagnostic {
            severity: spec.effective_severity,
            code: entry.code,
            title: substitute(&entry.title, &spec.args)?,
            message,
            labels,
            causes,
            help,
        })
    }
}

/// Reads `[diagnostics].dir` from the supported strict config subset.
///
/// # Errors
///
/// Returns an error for malformed, duplicate or unknown diagnostics settings.
pub fn diagnostic_directory_from_config(
    text: &str,
    config_path: &Path,
) -> Result<Option<std::path::PathBuf>, String> {
    let mut section = "";
    let mut directory = None;
    for raw in text.lines() {
        let line = strip_config_comment(raw)?.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if section != "diagnostics" {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("malformed diagnostics configuration: {line}"))?;
        if key.trim() != "dir" {
            return Err(format!("unknown diagnostics key: {}", key.trim()));
        }
        if directory.is_some() {
            return Err("duplicate diagnostics configuration key: dir".into());
        }
        let path = std::path::PathBuf::from(parse_config_string(value.trim())?);
        directory = Some(if path.is_absolute() {
            path
        } else {
            config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        });
    }
    Ok(directory)
}

fn validate_catalog_entry(entry: &CatalogEntry) -> Result<(), String> {
    let schema = entry.id.argument_schema();
    for pattern in std::iter::once(&entry.message)
        .chain(std::iter::once(&entry.title))
        .chain(entry.label.iter())
        .chain(entry.label_secondary.iter())
        .chain(entry.causes.iter())
        .chain(entry.help.iter())
    {
        for argument in pattern_arguments(pattern)? {
            if !schema.iter().any(|expected| expected.name == argument) {
                return Err(format!(
                    "unknown catalog argument for {}: {argument}",
                    entry.id.code()
                ));
            }
        }
    }
    Ok(())
}

fn pattern_arguments(pattern: &str) -> Result<Vec<&str>, String> {
    let mut arguments = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = pattern[cursor..].find("{$") {
        let start = cursor + relative + 2;
        let end = pattern[start..]
            .find('}')
            .map(|offset| start + offset)
            .ok_or_else(|| format!("unterminated Fluent argument in {pattern}"))?;
        arguments.push(&pattern[start..end]);
        cursor = end + 1;
    }
    Ok(arguments)
}

/// Renders the human-readable message for a structured spec from its arguments.
/// An explicit `message` argument wins; otherwise a per-code shape is used, and
/// the fallback joins all argument values (so `detail`-only codes render their
/// detail verbatim).
fn render_structured_message(spec: &DiagnosticSpec) -> String {
    spec.args
        .iter()
        .find(|(name, _)| name == "message")
        .map_or_else(
            || {
                let value = |name: &str| {
                    spec.args
                        .iter()
                        .find(|(argument, _)| argument == name)
                        .map_or_else(|| "<unknown>".to_string(), |(_, value)| value.to_string())
                };
                match spec.id {
                    DiagId::Lexical => {
                        format!("found '{}', expected {}", value("found"), value("expected"))
                    }
                    DiagId::Parse => {
                        format!("expected {} in {}", value("expected"), value("context"))
                    }
                    _ => spec
                        .args
                        .iter()
                        .map(|(_, value)| value.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                }
            },
            |(_, value)| value.to_string(),
        )
}

fn validate_arguments(spec: &DiagnosticSpec) -> Result<(), String> {
    let schema = spec.id.argument_schema();
    let mut seen = HashSet::new();
    for (name, value) in &spec.args {
        if !seen.insert(name.as_str()) {
            return Err(format!("duplicate diagnostic argument: {name}"));
        }
        let expected = schema
            .iter()
            .find(|argument| argument.name == name)
            .ok_or_else(|| format!("unknown diagnostic argument for {}: {name}", spec.id.code()))?;
        if value.kind() != expected.kind {
            return Err(format!(
                "invalid type for diagnostic argument {}.{name}: expected {:?}, got {:?}",
                spec.id.code(),
                expected.kind,
                value.kind()
            ));
        }
    }
    if let Some(missing) = schema.iter().find(|argument| !seen.contains(argument.name)) {
        return Err(format!(
            "missing diagnostic argument for {}: {}",
            spec.id.code(),
            missing.name
        ));
    }
    Ok(())
}

fn parse_shard(text: &str) -> Result<Vec<CatalogEntry>, String> {
    let mut entries = Vec::new();
    let mut current: Option<CatalogEntryBuilder> = None;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            let (id, value) = line
                .split_once('=')
                .ok_or_else(|| format!("malformed Fluent message: {line}"))?;
            if let Some(builder) = current.take() {
                entries.push(builder.finish()?);
            }
            let id = DiagId::from_fluent_id(id.trim())
                .ok_or_else(|| format!("unknown Fluent diagnostic id: {}", id.trim()))?;
            current = Some(CatalogEntryBuilder::new(id, value.trim()));
            continue;
        }
        let builder = current
            .as_mut()
            .ok_or_else(|| format!("attribute without message: {line}"))?;
        let attribute = line.trim();
        if !attribute.starts_with('.') {
            builder.continuation(attribute)?;
            continue;
        }
        let (name, value) = attribute
            .split_once('=')
            .ok_or_else(|| format!("malformed Fluent attribute: {line}"))?;
        builder.attribute(name.trim().trim_start_matches('.'), value.trim())?;
    }
    if let Some(builder) = current {
        entries.push(builder.finish()?);
    }
    Ok(entries)
}

#[derive(Default)]
struct CatalogEntryBuilder {
    id: Option<DiagId>,
    message: String,
    title: Option<String>,
    code: Option<&'static str>,
    label: Option<String>,
    label_secondary: Option<String>,
    causes: Vec<String>,
    help: Option<String>,
    last_field: Option<&'static str>,
    attributes: HashSet<String>,
}

impl CatalogEntryBuilder {
    fn new(id: DiagId, message: &str) -> Self {
        Self {
            id: Some(id),
            message: message.into(),
            last_field: Some("message"),
            ..Self::default()
        }
    }

    fn attribute(&mut self, name: &str, value: &str) -> Result<(), String> {
        if !self.attributes.insert(name.into()) {
            return Err(format!("duplicate Fluent attribute: {name}"));
        }
        match name {
            "title" => self.title = Some(value.into()),
            "code" => {
                if value != self.id.expect("builder id").code() {
                    return Err(format!("Fluent code does not match registry: {value}"));
                }
                self.code = Some(self.id.expect("builder id").code());
            }
            "label" => self.label = Some(value.into()),
            "label_secondary" => self.label_secondary = Some(value.into()),
            "cause" | "cause2" | "cause3" => self.causes.push(value.into()),
            "help" => self.help = Some(value.into()),
            _ => return Err(format!("unknown Fluent attribute: {name}")),
        }
        self.last_field = Some(match name {
            "title" => "title",
            "code" => "code",
            "label" => "label",
            "label_secondary" => "label_secondary",
            "cause" | "cause2" | "cause3" => "cause",
            "help" => "help",
            _ => unreachable!("attribute names checked above"),
        });
        Ok(())
    }

    fn continuation(&mut self, value: &str) -> Result<(), String> {
        let target = match self.last_field {
            Some("message") => &mut self.message,
            Some("title") => self.title.as_mut().expect("title was assigned"),
            Some("label") => self.label.as_mut().expect("label was assigned"),
            Some("label_secondary") => self
                .label_secondary
                .as_mut()
                .expect("secondary label was assigned"),
            Some("cause") => self.causes.last_mut().expect("cause was assigned"),
            Some("help") => self.help.as_mut().expect("help was assigned"),
            Some("code") => return Err("Fluent code attribute cannot be multiline".into()),
            _ => return Err("Fluent continuation has no preceding value".into()),
        };
        target.push('\n');
        target.push_str(value);
        Ok(())
    }

    fn finish(self) -> Result<CatalogEntry, String> {
        let id = self.id.expect("builder id");
        if self.message.trim().is_empty() {
            return Err(format!("empty message for {}", id.code()));
        }
        if self
            .title
            .as_deref()
            .is_none_or(|title| title.trim().is_empty())
        {
            return Err(format!("empty or missing title for {}", id.code()));
        }
        Ok(CatalogEntry {
            id,
            code: self
                .code
                .ok_or_else(|| format!("missing code for {}", id.code()))?,
            title: self.title.expect("non-empty title checked above"),
            message: self.message,
            label: self.label,
            label_secondary: self.label_secondary,
            causes: self.causes,
            help: self.help,
        })
    }
}

fn substitute(pattern: &str, args: &[(String, DiagnosticValue)]) -> Result<String, String> {
    let mut output = pattern.to_owned();
    let mut cursor = 0;
    while let Some(relative) = output[cursor..].find("{$") {
        let start = cursor + relative;
        let end = output[start..]
            .find('}')
            .map(|offset| start + offset)
            .ok_or_else(|| format!("unterminated Fluent argument in {pattern}"))?;
        let name = &output[start + 2..end];
        let value = args
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.to_string())
            .ok_or_else(|| format!("missing Fluent argument: {name}"))?;
        output.replace_range(start..=end, &value);
        cursor = start + value.len();
    }
    Ok(output)
}

#[derive(Default)]
pub struct DiagnosticSink {
    specs: Vec<DiagnosticSpec>,
    levels: HashMap<DiagId, Level>,
    seen: HashSet<(DiagId, Span)>,
}

impl DiagnosticSink {
    /// Emits a cheap, unformatted diagnostic specification.
    pub fn emit(
        &mut self,
        id: DiagId,
        args: Vec<(String, DiagnosticValue)>,
        labels: Vec<Label>,
    ) -> bool {
        let level = self
            .levels
            .get(&id)
            .copied()
            .unwrap_or(match id.severity() {
                Severity::Error => Level::Error,
                Severity::Warning => Level::Warn,
            });
        if level == Level::Allow {
            return false;
        }
        if let Some(primary) = labels
            .iter()
            .find(|label| label.style == LabelStyle::Primary)
            && !self.seen.insert((id, primary.span))
        {
            return false;
        }
        let effective_severity = match level {
            Level::Allow => unreachable!("allowed diagnostics return before storage"),
            Level::Warn => Severity::Warning,
            Level::Error => Severity::Error,
        };
        self.specs.push(DiagnosticSpec {
            id,
            effective_severity,
            args,
            labels,
        });
        true
    }

    /// Applies an effective warning level. Hard errors cannot be suppressed.
    ///
    /// # Errors
    ///
    /// Returns an error when `Level::Allow` is requested for a hard diagnostic.
    pub fn set_level(&mut self, id: DiagId, level: Level) -> Result<(), &'static str> {
        if level == Level::Allow && !id.warnings_allowed() {
            return Err("hard diagnostics cannot be allowed");
        }
        self.levels.insert(id, level);
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    #[must_use]
    pub fn specs(&self) -> &[DiagnosticSpec] {
        &self.specs
    }
}

#[derive(Debug)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: Box<str>,
    pub span: Span,
    pub structured: Option<Box<DiagnosticSpec>>,
}

impl Diagnostic {
    #[must_use]
    pub fn lexical(message: impl Into<String>, span: Span) -> Self {
        Self {
            code: "E0001",
            message: message.into().into_boxed_str(),
            span,
            structured: None,
        }
    }

    /// # Errors
    ///
    /// Returns an error when the labels or typed arguments violate the
    /// registered diagnostic contract.
    pub fn structured(
        id: DiagId,
        args: Vec<(String, DiagnosticValue)>,
        labels: Vec<Label>,
    ) -> Result<Self, String> {
        let span = labels
            .iter()
            .find(|label| label.style == LabelStyle::Primary)
            .ok_or_else(|| format!("diagnostic {} has no primary label", id.code()))?
            .span;
        let spec = DiagnosticSpec {
            id,
            effective_severity: id.severity(),
            args,
            labels,
        };
        validate_arguments(&spec)?;
        let message = render_structured_message(&spec);
        Ok(Self {
            code: id.code(),
            message: message.into_boxed_str(),
            span,
            structured: Some(Box::new(spec)),
        })
    }

    /// Builds a source-less structured diagnostic: no primary label and a
    /// synthetic zero span, for tool-boundary producers (e.g. `bnc`) that carry
    /// structured facts but have no BN source identity. Renders identically to
    /// [`Self::structured`].
    ///
    /// # Errors
    ///
    /// Returns an error when the arguments violate the code's schema.
    pub fn structured_source_less(
        id: DiagId,
        args: Vec<(String, DiagnosticValue)>,
    ) -> Result<Self, String> {
        let spec = DiagnosticSpec {
            id,
            effective_severity: id.severity(),
            args,
            labels: Vec::new(),
        };
        validate_arguments(&spec)?;
        let message = render_structured_message(&spec);
        let zero = Position {
            source_id: SourceId(0),
            revision: Revision(0),
            offset: 0,
            line: 0,
            column: 0,
        };
        Ok(Self {
            code: id.code(),
            message: message.into_boxed_str(),
            span: Span {
                start: zero,
                end: zero,
            },
            structured: Some(Box::new(spec)),
        })
    }

    /// Creates a lexical diagnostic from the offending token and expectation.
    ///
    /// # Errors
    ///
    /// Returns an error when either required lexical fact is absent or has the
    /// wrong type.
    pub fn lexical_facts(
        found: impl Into<String>,
        expected: impl Into<String>,
        span: Span,
    ) -> Result<Self, String> {
        Self::structured(
            DiagId::Lexical,
            vec![
                ("found".into(), found.into().into()),
                ("expected".into(), expected.into().into()),
            ],
            vec![Label {
                span,
                style: LabelStyle::Primary,
                text: None,
            }],
        )
    }

    /// Creates a parser diagnostic from the required token and syntactic
    /// context.
    ///
    /// # Errors
    ///
    /// Returns an error when the parser facts do not satisfy the registry
    /// contract.
    pub fn parse_facts(
        expected: impl Into<String>,
        context: impl Into<String>,
        span: Span,
    ) -> Result<Self, String> {
        Self::structured(
            DiagId::Parse,
            vec![
                ("expected".into(), expected.into().into()),
                ("context".into(), context.into().into()),
            ],
            vec![Label {
                span,
                style: LabelStyle::Primary,
                text: None,
            }],
        )
    }

    /// Creates a diagnostic from facts that are independent of presentation.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no primary label or the arguments do not
    /// match the registry schema for `id`.
    /// Converts diagnostics whose legacy code is already in the registry into
    /// the structured form consumed by the Fluent catalog.
    #[must_use]
    pub fn spec(&self) -> Option<DiagnosticSpec> {
        if let Some(spec) = &self.structured {
            return Some((**spec).clone());
        }
        let id = DiagId::from_code(self.code)?;
        if id == DiagId::Lexical {
            return Some(DiagnosticSpec {
                id,
                effective_severity: id.severity(),
                args: vec![
                    ("found".into(), self.message.to_string().into()),
                    ("expected".into(), "a valid Basic Next token".into()),
                ],
                labels: vec![Label {
                    span: self.span,
                    style: LabelStyle::Primary,
                    text: None,
                }],
            });
        }
        if id == DiagId::Parse {
            return Some(DiagnosticSpec {
                id,
                effective_severity: id.severity(),
                args: vec![
                    (
                        "expected".into(),
                        self.message
                            .strip_prefix("expected ")
                            .unwrap_or(self.message.as_ref())
                            .to_string()
                            .into(),
                    ),
                    (
                        "context".into(),
                        "the current declaration or statement".into(),
                    ),
                ],
                labels: vec![Label {
                    span: self.span,
                    style: LabelStyle::Primary,
                    text: None,
                }],
            });
        }
        if id == DiagId::Runtime("INDEX_OUT_OF_BOUNDS") {
            return Some(DiagnosticSpec {
                id,
                effective_severity: id.severity(),
                args: vec![
                    ("index".into(), "unknown".into()),
                    ("bound".into(), "unknown".into()),
                    ("context".into(), self.message.to_string().into()),
                ],
                labels: vec![Label {
                    span: self.span,
                    style: LabelStyle::Primary,
                    text: None,
                }],
            });
        }
        if id == DiagId::TypeMismatch {
            return Some(DiagnosticSpec {
                id,
                effective_severity: id.severity(),
                args: vec![
                    ("expected".into(), "the operation's expected type".into()),
                    ("actual".into(), "an incompatible value".into()),
                    ("context".into(), self.message.to_string().into()),
                ],
                labels: vec![Label {
                    span: self.span,
                    style: LabelStyle::Primary,
                    text: None,
                }],
            });
        }
        Some(DiagnosticSpec {
            id,
            effective_severity: id.severity(),
            args: vec![("message".into(), self.message.to_string().into())],
            labels: vec![Label {
                span: self.span,
                style: LabelStyle::Primary,
                text: None,
            }],
        })
    }

    /// Renders through the structured catalog when this legacy code has a
    /// registered mapping, preserving the old renderer for unmigrated codes.
    #[must_use]
    pub fn render_with_catalog(&self, source: &SourceFile, catalog: &Catalog) -> String {
        let Some(spec) = self.spec() else {
            return self.render(source);
        };
        let Ok(rendered) = catalog.render(&spec) else {
            return self.render(source);
        };
        render_catalog_diagnostic(source, &rendered)
    }

    /// Applies the effective warning level before rendering a catalog-backed
    /// diagnostic. Hard errors remain renderable at `Level::Error`.
    #[must_use]
    pub fn render_with_catalog_and_policy(
        &self,
        source: &SourceFile,
        catalog: &Catalog,
        policy: &WarningPolicy,
    ) -> String {
        let Some(spec) = self.spec() else {
            return self.render(source);
        };
        match policy.level(spec.id) {
            Level::Allow => String::new(),
            Level::Warn => self.render_with_catalog(source, catalog),
            Level::Error => {
                let mut promoted = spec;
                promoted.effective_severity = Severity::Error;
                catalog.render(&promoted).map_or_else(
                    |_| self.render(source),
                    |rendered| render_catalog_diagnostic(source, &rendered),
                )
            }
        }
    }

    #[must_use]
    pub fn render(&self, source: &SourceFile) -> String {
        let position = self.span.start;
        format!(
            "error[{code}]: {message}\n --> {name}:{line}:{column}\n  |\n{line:>3} | {text}\n  | {padding}^",
            code = self.code,
            message = self.message,
            name = source.name,
            line = position.line,
            column = position.column,
            text = source.line(position.line),
            padding = " ".repeat(position.column.saturating_sub(1))
        )
    }
}

fn render_catalog_diagnostic(source: &SourceFile, rendered: &RenderedDiagnostic) -> String {
    let position = rendered.labels.first().map_or(
        Position {
            source_id: source.source_id,
            revision: source.revision,
            offset: 0,
            line: 1,
            column: 1,
        },
        |label| label.span.start,
    );
    let label = rendered
        .labels
        .first()
        .and_then(|label| label.text.as_deref())
        .unwrap_or("");
    format!(
        "{severity}[{code}]: {title}: {message}\n --> {name}:{line}:{column}\n  |\n{line:>3} | {text}\n  | {padding}^{label}",
        severity = match rendered.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        },
        code = rendered.code,
        title = rendered.title,
        message = rendered.message,
        name = source.name,
        line = position.line,
        column = position.column,
        text = source.line(position.line),
        padding = " ".repeat(position.column.saturating_sub(1)),
        label = if label.is_empty() {
            String::new()
        } else {
            format!(" {label}")
        },
    )
}

#[cfg(test)]
mod tests {
    use bn_source::{Position, Revision, SourceId, Span};

    use bn_source::SourceFile;

    use super::{
        Catalog, DiagId, Diagnostic, DiagnosticSink, Label, LabelStyle, Level, Severity,
        WarningPolicy,
    };

    fn span(offset: usize) -> Span {
        let position = Position {
            source_id: SourceId(1),
            revision: Revision(1),
            offset,
            line: 1,
            column: offset + 1,
        };
        Span {
            start: position,
            end: position,
        }
    }

    #[test]
    fn registry_exposes_stable_codes_and_severities() {
        assert_eq!(DiagId::TypeMismatch.code(), "TYPE_MISMATCH");
        assert_eq!(DiagId::TypeMismatch.severity(), Severity::Error);
        assert_eq!(DiagId::UnusedBinding.severity(), Severity::Warning);
        assert!(DiagId::UnusedBinding.warnings_allowed());
        assert!(!DiagId::TypeMismatch.warnings_allowed());
    }

    #[test]
    fn registry_classifies_current_runtime_and_bnc_codes() {
        for code in [
            "BNC",
            "BNC_ENGINE",
            "DOUBLE_RELEASE",
            "FUNCTION_NOT_FOUND",
            "USE_AFTER_RELEASE",
        ] {
            let id = DiagId::from_code(code)
                .unwrap_or_else(|| panic!("current producer code {code} must be registered"));
            assert_eq!(id.code(), code);
            assert_eq!(id.severity(), Severity::Error);
        }
    }

    #[test]
    fn typed_diagnostic_values_render_deterministically() {
        let rendered = super::substitute(
            "signed={$signed}; unsigned={$unsigned}; enabled={$enabled}; name={$name}",
            &[
                ("signed".into(), (-7_i64).into()),
                ("unsigned".into(), 9_u64.into()),
                ("enabled".into(), true.into()),
                ("name".into(), "value".into()),
            ],
        )
        .expect("typed substitution");
        assert_eq!(rendered, "signed=-7; unsigned=9; enabled=true; name=value");
    }

    #[test]
    fn catalog_rejects_arguments_outside_the_identity_schema() {
        let catalog = Catalog::embedded_en_us().expect("embedded catalog");
        let base = |args| super::DiagnosticSpec {
            id: DiagId::TypeMismatch,
            effective_severity: Severity::Error,
            args,
            labels: Vec::new(),
        };
        assert!(catalog.render(&base(Vec::new())).is_err());
        assert!(
            catalog
                .render(&base(vec![("unknown".into(), "value".into())]))
                .is_err()
        );
        assert!(
            catalog
                .render(&base(vec![("message".into(), 1_u64.into())]))
                .is_err()
        );
        assert!(
            catalog
                .render(&base(vec![
                    ("message".into(), "first".into()),
                    ("message".into(), "second".into()),
                ]))
                .is_err()
        );
    }

    #[test]
    fn warning_policy_obeys_config_global_and_cli_precedence() {
        let mut policy = WarningPolicy::from_config(
            "[warnings]\ndefault = \"error\"\n[warnings.levels]\nUNUSED_BINDING = \"allow\"\n",
        )
        .expect("warning config");
        assert_eq!(policy.level(DiagId::UnusedBinding), Level::Allow);
        assert_eq!(policy.level(DiagId::UnusedImport), Level::Error);
        policy.set_warnings_as_errors(true);
        assert_eq!(policy.level(DiagId::UnusedBinding), Level::Error);
        policy
            .set_cli(DiagId::UnusedBinding, Level::Allow)
            .expect("CLI override");
        assert_eq!(policy.level(DiagId::UnusedBinding), Level::Allow);
        assert_eq!(policy.level(DiagId::TypeMismatch), Level::Error);
    }

    #[test]
    fn warning_policy_rejects_invalid_codes_levels_and_hard_allow() {
        assert!(WarningPolicy::from_config("[warnings.levels]\nUNKNOWN = \"warn\"\n").is_err());
        assert!(WarningPolicy::from_config("[warnings]\ndefault = \"allow\"\n").is_err());
        assert!(
            WarningPolicy::from_config("[warnings.levels]\nTYPE_MISMATCH = \"allow\"\n").is_err()
        );
        assert!(WarningPolicy::from_config("[warnings]\ndefault = \"maybe\"\n").is_err());
        assert!(WarningPolicy::from_config("[warnings]\nunknown = \"warn\"\n").is_err());
        assert!(
            WarningPolicy::from_config("[warnings]\ndefault = \"warn\"\ndefault = \"error\"\n")
                .is_err()
        );
    }

    #[test]
    fn warning_config_preserves_hash_inside_quoted_values() {
        let error = WarningPolicy::from_config("[warnings]\ndefault = \"warn#still-value\"\n")
            .expect_err("quoted hash belongs to the value");
        assert!(error.contains("warn#still-value"));
    }

    #[test]
    fn sink_deduplicates_before_rendering_and_can_allow_warnings() {
        let primary = Label {
            span: span(3),
            style: LabelStyle::Primary,
            text: None,
        };
        let mut sink = DiagnosticSink::default();
        sink.emit(DiagId::TypeMismatch, Vec::new(), vec![primary.clone()]);
        sink.emit(DiagId::TypeMismatch, Vec::new(), vec![primary]);
        assert_eq!(sink.len(), 1);
        sink.set_level(DiagId::UnusedBinding, Level::Allow)
            .expect("warnings may be allowed");
        sink.emit(
            DiagId::UnusedBinding,
            vec![("name".into(), "temporary".into())],
            vec![Label {
                span: span(5),
                style: LabelStyle::Primary,
                text: None,
            }],
        );
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn sink_does_not_merge_different_sources_or_revisions() {
        let mut sink = DiagnosticSink::default();
        for (source_id, revision) in [(1, 1), (2, 1), (1, 2)] {
            let position = Position {
                source_id: SourceId(source_id),
                revision: Revision(revision),
                offset: 3,
                line: 1,
                column: 4,
            };
            sink.emit(
                DiagId::TypeMismatch,
                Vec::new(),
                vec![Label {
                    span: Span {
                        start: position,
                        end: position,
                    },
                    style: LabelStyle::Primary,
                    text: None,
                }],
            );
        }
        assert_eq!(sink.len(), 3);
    }

    #[test]
    fn sink_retains_effective_severity_as_structured_data() {
        let mut sink = DiagnosticSink::default();
        sink.set_level(DiagId::UnusedBinding, Level::Error)
            .expect("warnings may be promoted");
        sink.emit(
            DiagId::UnusedBinding,
            vec![("name".into(), "temporary".into())],
            vec![Label {
                span: span(1),
                style: LabelStyle::Primary,
                text: None,
            }],
        );
        let spec = &sink.specs()[0];
        assert_eq!(spec.id.severity(), Severity::Warning);
        assert_eq!(spec.effective_severity, Severity::Error);
        let rendered = Catalog::embedded_en_us()
            .expect("embedded catalog")
            .render(spec)
            .expect("rendered diagnostic");
        assert_eq!(rendered.severity, Severity::Error);
    }

    #[test]
    fn hard_errors_cannot_be_suppressed() {
        let mut sink = DiagnosticSink::default();
        assert!(sink.set_level(DiagId::TypeMismatch, Level::Allow).is_err());
    }

    #[test]
    fn embedded_catalog_covers_registry_and_renders_arguments_lazily() {
        let catalog = super::Catalog::embedded_en_us().expect("embedded catalog");
        assert_eq!(catalog.len(), DiagId::all().count());
        let span = span(7);
        let spec = super::DiagnosticSpec {
            id: DiagId::UnusedBinding,
            effective_severity: Severity::Warning,
            args: vec![("name".into(), "temporary".into())],
            labels: vec![Label {
                span,
                style: LabelStyle::Primary,
                text: None,
            }],
        };
        let rendered = catalog.render(&spec).expect("render spec");
        assert_eq!(rendered.code, "UNUSED_BINDING");
        assert_eq!(rendered.severity, Severity::Warning);
        assert!(rendered.message.contains("temporary"));
        assert_eq!(rendered.labels.len(), 1);

        let division = super::DiagnosticSpec {
            id: DiagId::Runtime("DIVISION_BY_ZERO"),
            effective_severity: Severity::Error,
            args: vec![("operation".into(), "DIV".into())],
            labels: vec![Label {
                span,
                style: LabelStyle::Primary,
                text: None,
            }],
        };
        let rendered_division = catalog.render(&division).expect("render division error");
        assert_eq!(rendered_division.causes.len(), 1);
        assert!(rendered_division.help.is_some());

        let missing = super::DiagnosticSpec {
            id: DiagId::Runtime("NAME_NOT_FOUND"),
            effective_severity: Severity::Error,
            args: vec![
                ("name".into(), "missingField".into()),
                ("context".into(), "object member".into()),
            ],
            labels: vec![Label {
                span,
                style: LabelStyle::Primary,
                text: None,
            }],
        };
        let rendered_missing = catalog.render(&missing).expect("render missing name");
        assert!(rendered_missing.message.contains("missingField"));
        assert_eq!(rendered_missing.causes.len(), 1);
        assert!(rendered_missing.help.is_some());
    }

    #[test]
    fn catalog_rejects_duplicate_ids_and_missing_required_attributes() {
        let duplicate = "type-mismatch = first\n    .title = Type mismatch\n    .code = TYPE_MISMATCH\ntype-mismatch = second\n";
        assert!(super::Catalog::from_ftl(duplicate).is_err());
        let missing_title = "type-mismatch = message\n    .code = TYPE_MISMATCH\n";
        assert!(super::Catalog::from_ftl(missing_title).is_err());
        assert!(
            super::parse_shard(
                "type-mismatch =   \n    .title = Title\n    .code = TYPE_MISMATCH\n"
            )
            .is_err()
        );
        assert!(
            super::parse_shard(
                "type-mismatch = message\n    .title =   \n    .code = TYPE_MISMATCH\n"
            )
            .is_err()
        );
    }

    #[test]
    fn fluent_subset_rejects_duplicate_attributes_and_unknown_arguments() {
        let duplicate = "type-mismatch = {$message}\n    .title = First\n    .title = Second\n    .code = TYPE_MISMATCH\n";
        assert!(
            super::parse_shard(duplicate)
                .expect_err("duplicate title")
                .contains("duplicate Fluent attribute: title")
        );
        let unknown =
            "type-mismatch = {$unknown}\n    .title = Type mismatch\n    .code = TYPE_MISMATCH\n";
        assert!(super::parse_shard(unknown).is_ok());
        assert!(
            super::Catalog::from_ftl(unknown)
                .expect_err("unknown argument")
                .contains("unknown catalog argument")
        );
    }

    #[test]
    fn fluent_subset_supports_multiline_values_and_catalog_label_defaults() {
        let entries = super::parse_shard(
            "type-mismatch = first {$expected}\n    second line\n    .title = Type mismatch\n    .code = TYPE_MISMATCH\n    .label = expected {$expected}\n      here\n    .label_secondary = declared here\n",
        )
        .expect("supported multiline Fluent subset");
        assert_eq!(entries[0].message, "first {$expected}\nsecond line");
        assert_eq!(
            entries[0].label.as_deref(),
            Some("expected {$expected}\nhere")
        );

        let catalog = Catalog::embedded_en_us().expect("embedded catalog");
        let diagnostic = super::DiagnosticSpec {
            id: DiagId::TypeMismatch,
            effective_severity: Severity::Error,
            args: vec![
                ("expected".into(), "INTEGER".into()),
                ("actual".into(), "STRING".into()),
                ("context".into(), "assignment".into()),
            ],
            labels: vec![
                Label {
                    span: span(1),
                    style: LabelStyle::Primary,
                    text: None,
                },
                Label {
                    span: span(2),
                    style: LabelStyle::Secondary,
                    text: Some("producer detail".into()),
                },
            ],
        };
        let rendered = catalog.render(&diagnostic).expect("render labels");
        assert_eq!(
            rendered.labels[0].text.as_deref(),
            Some("incompatible value")
        );
        assert_eq!(rendered.labels[1].text.as_deref(), Some("producer detail"));
    }

    #[test]
    fn embedded_catalog_is_loaded_once_and_reused() {
        let first = super::Catalog::embedded_global();
        let second = super::Catalog::embedded_global();
        assert!(std::ptr::eq(first, second));
        assert_eq!(first.len(), DiagId::all().count());
    }

    #[test]
    fn overlay_replaces_only_present_entries_and_keeps_embedded_fallback() {
        use std::fs;

        let directory = std::env::temp_dir().join(format!(
            "bn-diag-overlay-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("overlay directory");
        fs::write(
            directory.join("lex.ftl"),
            "lexical-error = Found {$found}, expected {$expected}.\n    .title = Erro lexical personalizado\n    .code = E0001\n    .label = token inválido\n",
        )
        .expect("overlay shard");

        let catalog =
            super::Catalog::embedded_en_us_with_overlay(&directory).expect("overlay catalog");
        let lexical = super::DiagnosticSpec {
            id: DiagId::Lexical,
            effective_severity: Severity::Error,
            args: vec![
                ("found".into(), "bad token".into()),
                ("expected".into(), "a valid Basic Next token".into()),
            ],
            labels: vec![Label {
                span: span(0),
                style: LabelStyle::Primary,
                text: None,
            }],
        };
        assert_eq!(
            catalog.render(&lexical).expect("lexical render").title,
            "Erro lexical personalizado"
        );
        let parse = super::DiagnosticSpec {
            id: DiagId::Parse,
            effective_severity: Severity::Error,
            args: vec![
                ("expected".into(), "AS".into()),
                (
                    "context".into(),
                    "the current declaration or statement".into(),
                ),
            ],
            labels: Vec::new(),
        };
        assert_eq!(
            catalog.render(&parse).expect("embedded fallback").code,
            "E0100"
        );
        fs::remove_dir_all(directory).expect("remove overlay");
    }

    #[test]
    fn explicitly_selected_missing_overlay_is_an_error() {
        let directory =
            std::env::temp_dir().join(format!("bn-missing-diag-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let error = Catalog::embedded_en_us_with_overlay(&directory)
            .expect_err("explicitly selected missing overlay must fail");
        assert!(error.contains("not a readable directory"));
        assert!(error.contains(&directory.display().to_string()));
    }

    #[test]
    fn diagnostics_directory_is_relative_to_selected_config() {
        let config = std::path::Path::new("/project/config/config.toml");
        assert_eq!(
            super::diagnostic_directory_from_config(
                "[diagnostics]\ndir = \"catalogs/en-US#local\" # comment\n",
                config,
            )
            .expect("diagnostics config"),
            Some(std::path::PathBuf::from(
                "/project/config/catalogs/en-US#local"
            ))
        );
        assert!(
            super::diagnostic_directory_from_config(
                "[diagnostics]\ndir = \"one\"\ndir = \"two\"\n",
                config,
            )
            .is_err()
        );
        assert!(super::diagnostic_directory_from_config(
            "[diagnostics]\nunknown = \"value\"\n",
            config,
        )
        .is_err());
    }

    #[test]
    fn catalog_path_precedence_is_environment_config_install_embedded() {
        let root = std::env::temp_dir().join(format!("bn-diag-precedence-{}", std::process::id()));
        let environment = root.join("environment");
        let configured = root.join("configured");
        let installed = root.join("installed");
        for (directory, title) in [
            (&environment, "Environment"),
            (&configured, "Configured"),
            (&installed, "Installed"),
        ] {
            std::fs::create_dir_all(directory).expect("overlay directory");
            std::fs::write(
                directory.join("lex.ftl"),
                format!("lexical-error = Found {{$found}}, expected {{$expected}}.\n    .title = {title}\n    .code = E0001\n"),
            )
            .expect("overlay fixture");
        }
        let title = |catalog: Catalog| {
            catalog
                .render(&super::DiagnosticSpec {
                    id: DiagId::Lexical,
                    effective_severity: Severity::Error,
                    args: vec![
                        ("found".into(), "bad token".into()),
                        ("expected".into(), "a valid Basic Next token".into()),
                    ],
                    labels: Vec::new(),
                })
                .expect("render")
                .title
        };
        assert_eq!(
            title(
                Catalog::selected_from_paths(
                    Some(&environment),
                    Some(&configured),
                    Some(&installed)
                )
                .expect("environment")
            ),
            "Environment"
        );
        assert_eq!(
            title(
                Catalog::selected_from_paths(None, Some(&configured), Some(&installed))
                    .expect("configured")
            ),
            "Configured"
        );
        assert_eq!(
            title(Catalog::selected_from_paths(None, None, Some(&installed)).expect("installed")),
            "Installed"
        );
        assert_eq!(
            title(Catalog::selected_from_paths(None, None, None).expect("embedded")),
            "Lexical error"
        );
        std::fs::remove_dir_all(root).expect("remove precedence fixtures");
    }

    #[test]
    fn legacy_lexical_diagnostic_bridges_to_catalog() {
        let source = SourceFile::new("main.bn", "@");
        let diagnostic = Diagnostic::lexical("invalid token", span(0));
        let spec = diagnostic.spec().expect("registered lexical code");
        assert_eq!(spec.id, DiagId::Lexical);
        assert_eq!(
            spec.args,
            vec![
                ("found".into(), "invalid token".into()),
                ("expected".into(), "a valid Basic Next token".into()),
            ]
        );
        let rendered = diagnostic.render_with_catalog(&source, super::Catalog::embedded_global());
        assert!(rendered.starts_with("error[E0001]: Lexical error: Found 'invalid token'"));
        assert!(rendered.contains("invalid token"));
    }

    #[test]
    fn typed_lexical_and_parser_diagnostics_keep_compatibility_text() {
        let lexical =
            Diagnostic::lexical_facts("§", "a Basic Next token", span(0)).expect("lexical facts");
        assert!(lexical.message.contains("§"));
        assert!(lexical.message.contains("a Basic Next token"));

        let parse =
            Diagnostic::parse_facts("AS", "a binding declaration", span(0)).expect("parser facts");
        assert!(parse.message.contains("AS"));
        assert!(parse.message.contains("binding declaration"));
    }

    #[test]
    fn legacy_parse_diagnostic_bridges_to_catalog() {
        let source = SourceFile::new("main.bn", "LET");
        let diagnostic = Diagnostic {
            code: "E0100",
            message: "expected AS".into(),
            span: span(0),
            structured: None,
        };
        let spec = diagnostic.spec().expect("registered parse code");
        assert_eq!(spec.id, DiagId::Parse);
        let rendered = diagnostic.render_with_catalog(&source, super::Catalog::embedded_global());
        assert!(rendered.starts_with("error[E0100]: Syntax error: Expected AS"));
    }

    #[test]
    fn catalog_renderer_preserves_unicode_lines_and_crlf_coordinates() {
        let source = SourceFile::new("unicode.bn", "LET α AS STRING\r\nPRINT α\r\n");
        let position = Position {
            source_id: source.source_id,
            revision: source.revision,
            offset: 22,
            line: 2,
            column: 7,
        };
        let diagnostic = Diagnostic::structured(
            DiagId::TypeMismatch,
            vec![
                ("expected".into(), "INTEGER".into()),
                ("actual".into(), "STRING".into()),
                ("context".into(), "PRINT operand".into()),
            ],
            vec![Label {
                span: Span {
                    start: position,
                    end: position,
                },
                style: LabelStyle::Primary,
                text: None,
            }],
        )
        .expect("structured type mismatch");
        let rendered = diagnostic.render_with_catalog(&source, Catalog::embedded_global());
        assert!(rendered.contains("unicode.bn:2:7"));
        assert!(rendered.contains("PRINT α"));
        assert!(!rendered.contains("PRINT α\r"));
        assert!(rendered.contains("Expected INTEGER, but found STRING"));
    }

    #[test]
    fn legacy_runtime_diagnostic_bridges_to_catalog() {
        let diagnostic = Diagnostic {
            code: "INDEX_OUT_OF_BOUNDS",
            message: "index 4 is outside the vector".into(),
            span: span(2),
            structured: None,
        };
        let spec = diagnostic.spec().expect("runtime code is registered");
        assert_eq!(spec.id.code(), "INDEX_OUT_OF_BOUNDS");
        let rendered = Catalog::embedded_en_us()
            .expect("embedded catalog")
            .render(&spec)
            .expect("runtime catalog entry");
        assert_eq!(rendered.code, "INDEX_OUT_OF_BOUNDS");
        assert!(rendered.message.contains("index 4"));
    }
}
