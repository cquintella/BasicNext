// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stable diagnostic facts shared by frontend, IR and backend crates.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use bn_source::{SourceFile, Span};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagId {
    Lexical,
    Parse,
    TypeMismatch,
    NumericOverflow,
    InvalidIr,
    IrLowering,
    ModuleNotFound,
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
    const RUNTIME_CODES: &'static [&'static str] = &[
        "ALLOCATION_SIZE_INVALID",
        "ALLOCATION_SIZE_OVERFLOW",
        "ALLOCATION_TOO_LARGE",
        "BUILD_EMISSION_FAILED",
        "BUILD_TOOLCHAIN_UNAVAILABLE",
        "CONFIG_INVALID",
        "DEBUG_TERMINATED",
        "DISPATCH",
        "DIVISION_BY_ZERO",
        "DOUBLE_DELETE",
        "EXECUTION_POLICY_DENIED",
        "FORMAT_OUT_OF_RANGE",
        "HANDLER_NOT_FOUND",
        "HEADER_NOT_FOUND",
        "HOST_CAPABILITY_UNAVAILABLE",
        "INDEX_OUT_OF_BOUNDS",
        "INPUT_ERROR",
        "INVALID_EGRESS_POLICY",
        "INVALID_EXIT_CODE",
        "INVALID_EXPONENT",
        "INVALID_FOR_STEP",
        "INVALID_INPUT",
        "INVALID_JSON",
        "INVALID_NUMERIC_CONVERSION",
        "INVALID_OPTIONS",
        "INVALID_SHIFT_COUNT",
        "INVALID_START",
        "INVALID_VALUE",
        "IO",
        "LIMIT",
        "NAME_NOT_FOUND",
        "NOT_FOUND",
        "NULL_POINTER_ACCESS",
        "OUTPUT_ERROR",
        "POINTER_LENGTH_MISMATCH",
        "PROCESS_LOG_WRITE",
        "REQUEST_INVALID",
        "SCRAPER_INPUT",
        "SERVER_STATE",
        "SESSION_CONFIG",
        "STALE_HANDLE",
        "START_NOT_FOUND",
        "STATIC_INITIALIZATION_CYCLE",
        "TLS_PROVIDER_UNAVAILABLE",
        "UNINITIALIZED_VALUE",
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
    pub const fn severity(self) -> Severity {
        match self {
            Self::UnusedBinding | Self::UnusedImport | Self::UnreachableCode => Severity::Warning,
            Self::Lexical
            | Self::Parse
            | Self::TypeMismatch
            | Self::NumericOverflow
            | Self::InvalidIr
            | Self::IrLowering
            | Self::ModuleNotFound
            | Self::TargetUnsupportedEntrypoint
            | Self::TargetUnsupportedHost
            | Self::TargetUnsupportedOp
            | Self::TargetUnsupportedType
            | Self::TargetUnsupportedLlvm
            | Self::Runtime(_) => Severity::Error,
        }
    }

    #[must_use]
    pub const fn warnings_allowed(self) -> bool {
        matches!(self.severity(), Severity::Warning)
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
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or_default().trim();
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
            let value = value.trim().trim_matches('"');
            match section {
                "warnings" if key == "default" => {
                    let level = parse_level(value)?;
                    if level == Level::Allow {
                        return Err("warning default cannot be allow".into());
                    }
                    policy.config_default = Some(level);
                }
                "warnings.levels" => {
                    let id = DiagId::from_code(key)
                        .ok_or_else(|| format!("unknown diagnostic code in warnings: {key}"))?;
                    policy.set_config(id, parse_level(value)?)?;
                }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticSpec {
    pub id: DiagId,
    pub args: Vec<(String, String)>,
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
        match CATALOG.get_or_init(|| match std::env::var_os("BN_DIAGNOSTICS_DIR") {
            Some(directory) => Self::embedded_en_us_with_overlay(Path::new(&directory)),
            None => Self::embedded_en_us(),
        }) {
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
        Ok(RenderedDiagnostic {
            severity: spec.id.severity(),
            code: entry.code,
            title: substitute(&entry.title, &spec.args)?,
            message,
            labels: spec.labels.clone(),
            causes,
            help,
        })
    }
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
}

impl CatalogEntryBuilder {
    fn new(id: DiagId, message: &str) -> Self {
        Self {
            id: Some(id),
            message: message.into(),
            ..Self::default()
        }
    }

    fn attribute(&mut self, name: &str, value: &str) -> Result<(), String> {
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
        Ok(())
    }

    fn finish(self) -> Result<CatalogEntry, String> {
        let id = self.id.expect("builder id");
        Ok(CatalogEntry {
            id,
            code: self
                .code
                .ok_or_else(|| format!("missing code for {}", id.code()))?,
            title: self
                .title
                .ok_or_else(|| format!("missing title for {}", id.code()))?,
            message: self.message,
            label: self.label,
            label_secondary: self.label_secondary,
            causes: self.causes,
            help: self.help,
        })
    }
}

fn substitute(pattern: &str, args: &[(String, String)]) -> Result<String, String> {
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
            .map(|(_, value)| value)
            .ok_or_else(|| format!("missing Fluent argument: {name}"))?;
        output.replace_range(start..=end, value);
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
    pub fn emit(&mut self, id: DiagId, args: Vec<(String, String)>, labels: Vec<Label>) -> bool {
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
        self.specs.push(DiagnosticSpec { id, args, labels });
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
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    #[must_use]
    pub fn lexical(message: impl Into<String>, span: Span) -> Self {
        Self {
            code: "E0001",
            message: message.into(),
            span,
        }
    }

    /// Converts diagnostics whose legacy code is already in the registry into
    /// the structured form consumed by the Fluent catalog.
    #[must_use]
    pub fn spec(&self) -> Option<DiagnosticSpec> {
        let id = DiagId::from_code(self.code)?;
        Some(DiagnosticSpec {
            id,
            args: vec![("message".into(), self.message.clone())],
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
        let position = self.span.start;
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
            Level::Error => self
                .render_with_catalog(source, catalog)
                .replacen("warning[", "error[", 1),
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
    fn hard_errors_cannot_be_suppressed() {
        let mut sink = DiagnosticSink::default();
        assert!(sink.set_level(DiagId::TypeMismatch, Level::Allow).is_err());
    }

    #[test]
    fn embedded_catalog_covers_registry_and_renders_arguments_lazily() {
        let catalog = super::Catalog::embedded_en_us().expect("embedded catalog");
        assert_eq!(catalog.len(), 63);
        let span = span(7);
        let spec = super::DiagnosticSpec {
            id: DiagId::UnusedBinding,
            args: vec![
                ("name".into(), "temporary".into()),
                ("message".into(), "temporary is never read".into()),
            ],
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
    }

    #[test]
    fn catalog_rejects_duplicate_ids_and_missing_required_attributes() {
        let duplicate = "type-mismatch = first\n    .title = Type mismatch\n    .code = TYPE_MISMATCH\ntype-mismatch = second\n";
        assert!(super::Catalog::from_ftl(duplicate).is_err());
        let missing_title = "type-mismatch = message\n    .code = TYPE_MISMATCH\n";
        assert!(super::Catalog::from_ftl(missing_title).is_err());
    }

    #[test]
    fn embedded_catalog_is_loaded_once_and_reused() {
        let first = super::Catalog::embedded_global();
        let second = super::Catalog::embedded_global();
        assert!(std::ptr::eq(first, second));
        assert_eq!(first.len(), 63);
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
            "lexical-error = {$message}\n    .title = Erro lexical personalizado\n    .code = E0001\n    .label = token inválido\n",
        )
        .expect("overlay shard");

        let catalog =
            super::Catalog::embedded_en_us_with_overlay(&directory).expect("overlay catalog");
        let lexical = super::DiagnosticSpec {
            id: DiagId::Lexical,
            args: vec![("message".into(), "bad token".into())],
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
            args: vec![("message".into(), "expected AS".into())],
            labels: Vec::new(),
        };
        assert_eq!(
            catalog.render(&parse).expect("embedded fallback").code,
            "E0100"
        );
        fs::remove_dir_all(directory).expect("remove overlay");
    }

    #[test]
    fn legacy_lexical_diagnostic_bridges_to_catalog() {
        let source = SourceFile::new("main.bn", "@");
        let diagnostic = Diagnostic::lexical("invalid token", span(0));
        let spec = diagnostic.spec().expect("registered lexical code");
        assert_eq!(spec.id, DiagId::Lexical);
        assert_eq!(spec.args, vec![("message".into(), "invalid token".into())]);
        let rendered = diagnostic.render_with_catalog(&source, super::Catalog::embedded_global());
        assert!(rendered.starts_with("error[E0001]: Lexical error: invalid token"));
        assert!(rendered.contains("invalid token"));
    }

    #[test]
    fn legacy_parse_diagnostic_bridges_to_catalog() {
        let source = SourceFile::new("main.bn", "LET");
        let diagnostic = Diagnostic {
            code: "E0100",
            message: "expected AS".into(),
            span: span(0),
        };
        let spec = diagnostic.spec().expect("registered parse code");
        assert_eq!(spec.id, DiagId::Parse);
        let rendered = diagnostic.render_with_catalog(&source, super::Catalog::embedded_global());
        assert!(rendered.starts_with("error[E0100]: Syntax error: expected AS"));
    }

    #[test]
    fn legacy_runtime_diagnostic_bridges_to_catalog() {
        let diagnostic = Diagnostic {
            code: "INDEX_OUT_OF_BOUNDS",
            message: "index 4 is outside the vector".into(),
            span: span(2),
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
