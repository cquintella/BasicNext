use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const TARGETS: [&str; 3] = ["interpret", "llvm-native", "wasm32"];

#[derive(Debug)]
pub enum ReportError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Catalog(serde_json::Error),
    MissingEnum {
        name: String,
        path: PathBuf,
    },
    EmptyEnum {
        name: String,
        path: PathBuf,
    },
}

impl fmt::Display for ReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Catalog(source) => write!(formatter, "invalid capability catalog: {source}"),
            Self::MissingEnum { name, path } => {
                write!(
                    formatter,
                    "could not find enum {name} in {}",
                    path.display()
                )
            }
            Self::EmptyEnum { name, path } => {
                write!(
                    formatter,
                    "enum {name} in {} has no variants",
                    path.display()
                )
            }
        }
    }
}

impl Error for ReportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Catalog(source) => Some(source),
            Self::MissingEnum { .. } | Self::EmptyEnum { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Catalog {
    programs: Vec<CatalogProgram>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogProgram {
    id: String,
    target: String,
    ir_instructions: Vec<String>,
    type_constraints: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReportSource {
    pub instruction_model: String,
    pub type_model: String,
    pub catalog: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InventoryEntry {
    pub instruction: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub target: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub source: ReportSource,
    pub targets: Vec<String>,
    pub instruction_count: usize,
    pub type_count: usize,
    pub inventory_count: usize,
    pub covered_count: usize,
    pub gap_count: usize,
    pub inventory: Vec<InventoryEntry>,
}

impl Report {
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} combinations; {} covered; {} gaps",
            self.inventory_count, self.covered_count, self.gap_count
        )
    }
}

/// Builds the support inventory from the repository's IR, type, and catalog sources.
///
/// # Errors
///
/// Returns [`ReportError`] when a source cannot be read, the catalog is invalid
/// JSON, or either source enum is missing or has no variants.
pub fn build_report(repository_root: &Path) -> Result<Report, ReportError> {
    let instruction_model = repository_root.join("crates/bn_ir/src/model.rs");
    let type_model = repository_root.join("crates/bn_types/src/lib.rs");
    let catalog_path = repository_root.join("tests/compiler-capabilities.json");
    let instructions = enum_variants(&instruction_model, "Instruction")?;
    let types = enum_variants(&type_model, "Type")?;
    let catalog_source = read(&catalog_path)?;
    let catalog: Catalog = serde_json::from_str(&catalog_source).map_err(ReportError::Catalog)?;

    let mut evidence: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
    for row in catalog.programs {
        for instruction in row.ir_instructions {
            for type_name in &types {
                if row
                    .type_constraints
                    .iter()
                    .any(|constraint| constraint_matches(constraint, type_name))
                {
                    evidence
                        .entry((instruction.clone(), type_name.clone(), row.target.clone()))
                        .or_default()
                        .push(row.id.clone());
                }
            }
        }
    }

    let mut inventory = Vec::with_capacity(instructions.len() * types.len() * TARGETS.len());
    for instruction in &instructions {
        for type_name in &types {
            for target in TARGETS {
                let key = (instruction.clone(), type_name.clone(), target.to_owned());
                inventory.push(InventoryEntry {
                    instruction: instruction.clone(),
                    r#type: type_name.clone(),
                    target: target.to_owned(),
                    evidence: evidence.remove(&key).unwrap_or_default(),
                });
            }
        }
    }
    let gap_count = inventory
        .iter()
        .filter(|entry| entry.evidence.is_empty())
        .count();
    let inventory_count = inventory.len();

    Ok(Report {
        schema_version: 1,
        source: ReportSource {
            instruction_model: "crates/bn_ir/src/model.rs".to_owned(),
            type_model: "crates/bn_types/src/lib.rs".to_owned(),
            catalog: "tests/compiler-capabilities.json".to_owned(),
        },
        targets: TARGETS.into_iter().map(str::to_owned).collect(),
        instruction_count: instructions.len(),
        type_count: types.len(),
        inventory_count,
        covered_count: inventory_count - gap_count,
        gap_count,
        inventory,
    })
}

fn read(path: &Path) -> Result<String, ReportError> {
    fs::read_to_string(path).map_err(|source| ReportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn enum_variants(path: &Path, enum_name: &str) -> Result<Vec<String>, ReportError> {
    let source = read(path)?;
    let marker = format!("pub enum {enum_name} {{");
    let Some(body) = source.split_once(&marker).map(|(_, body)| body) else {
        return Err(ReportError::MissingEnum {
            name: enum_name.to_owned(),
            path: path.to_owned(),
        });
    };
    let mut variants = Vec::new();
    for line in body.lines() {
        if line == "}" {
            break;
        }
        let Some(candidate) = line.strip_prefix("    ") else {
            continue;
        };
        let name: String = candidate
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if name.starts_with(char::is_uppercase) {
            variants.push(name);
        }
    }
    if variants.is_empty() {
        return Err(ReportError::EmptyEnum {
            name: enum_name.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(variants)
}

#[must_use]
pub fn constraint_matches(constraint: &str, type_name: &str) -> bool {
    let normalized = constraint.strip_suffix(" ABI").unwrap_or(constraint);
    match type_name {
        "Integer" => matches!(
            normalized,
            "INTEGER" | "BYTE" | "INT8" | "INT16" | "INT64" | "UINT16" | "UINT32" | "UINT64"
        ),
        "Float" => matches!(normalized, "FLOAT" | "FLOAT32" | "FLOAT64"),
        "FloatLiteral" => normalized == "FLOAT_LITERAL",
        "Vector" => normalized.contains('['),
        "Boolean" => normalized == "BOOLEAN",
        "String" => normalized == "STRING",
        "EndOfFile" => normalized == "EOF",
        "HostNet" => normalized == "HOST.Net",
        "Named" => matches!(normalized, "TIMESTAMP" | "DATE" | "TIME"),
        _ => false,
    }
}
