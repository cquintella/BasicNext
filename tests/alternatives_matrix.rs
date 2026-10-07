// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Conformance matrix for alternative types (bucket typed-llvm-emitter,
//! Sprint 6 phase B). For every alternative of the matrix, one program stores
//! a value of each member through every flow (linear, parameter, return,
//! field, widening, literal; a vector element cannot be an alternative,
//! `vector-element-type` in 0.6.ebnf) and prints `IS` for every
//! member. The expected text comes from the language rule (`IS` holds only
//! for the member stored), not from either backend. Known failures are a
//! ratchet: a listed alternative must still fail, an unlisted one must pass.

mod support;

use std::{fmt::Write as _, time::Duration};

use bn_types::{
    FloatType, IntegerType, Type,
    literals::{NumericClass, numeric_alternative},
};
use support::{TestDir, bnc, bni, run};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Int32,
    Int64,
    Float32,
    Float64,
    Boolean,
    Str,
    Null,
    Na,
    Eof,
    Error,
    Object,
    /// `INT32[2]`: a vector can be a member, not an element type.
    Vector,
}

const KINDS: [Kind; 11] = [
    Kind::Int32,
    Kind::Int64,
    Kind::Float32,
    Kind::Float64,
    Kind::Boolean,
    Kind::Str,
    Kind::Null,
    Kind::Na,
    Kind::Eof,
    Kind::Error,
    Kind::Object,
];

impl Kind {
    /// The member as written in a type and in `IS`.
    const fn spelling(self) -> &'static str {
        match self {
            Self::Int32 => "INT32",
            Self::Int64 => "INT64",
            Self::Float32 => "FLOAT32",
            Self::Float64 => "FLOAT64",
            Self::Boolean => "BOOLEAN",
            Self::Str => "STRING",
            Self::Null => "NULL",
            Self::Na => "NA",
            Self::Eof => "EOF",
            Self::Error => "Error",
            Self::Object => "Box",
            Self::Vector => "INT32[2]",
        }
    }

    /// An expression of the member, valid inside the generated `Start`.
    const fn value(self) -> &'static str {
        match self {
            Self::Int32 => "k_int32",
            Self::Int64 => "k_int64",
            Self::Float32 => "k_float32",
            Self::Float64 => "k_float64",
            Self::Boolean => "k_boolean",
            Self::Str => "k_string",
            Self::Null => "NULL",
            Self::Na => "NA",
            Self::Eof => "k_eof",
            Self::Error => "k_error",
            Self::Object => "k_box",
            Self::Vector => "k_vector",
        }
    }

    /// An initializer a class field can use, if the member has one.
    const fn field_initializer(self) -> Option<&'static str> {
        match self {
            Self::Int32 | Self::Int64 => Some("1"),
            Self::Float32 | Self::Float64 => Some("1.5"),
            Self::Boolean => Some("FALSE"),
            Self::Str => Some("\"init\""),
            Self::Null => Some("NULL"),
            Self::Na => Some("NA"),
            Self::Object => Some("NEW Box()"),
            Self::Vector => Some("[1, 2]"),
            Self::Eof | Self::Error => None,
        }
    }

    /// What `PRINT` writes for the member value (console.md: integers,
    /// shortest floats, `TRUE`, unquoted strings, `NULL`, `NA`, `EOF`). `None`
    /// where the spec defines no text (objects, vectors) or where it depends
    /// on the host (`Error`: its `Cause`).
    const fn printed(self) -> Option<&'static str> {
        match self {
            Self::Int32 | Self::Int64 => Some("5"),
            Self::Float32 | Self::Float64 => Some("2.5"),
            Self::Boolean => Some("TRUE"),
            Self::Str => Some("s"),
            Self::Null => Some("NULL"),
            Self::Na => Some("NA"),
            Self::Eof => Some("EOF"),
            Self::Error | Self::Object | Self::Vector => None,
        }
    }

    /// A second value of the member, different from `value`, for `=`.
    const fn other_value(self) -> Option<&'static str> {
        match self {
            Self::Int32 => Some("k_int32_other"),
            Self::Int64 => Some("k_int64_other"),
            Self::Float32 => Some("k_float32_other"),
            Self::Float64 => Some("k_float64_other"),
            Self::Boolean => Some("k_boolean_other"),
            Self::Str => Some("k_string_other"),
            _ => None,
        }
    }

    /// Statements and an expression that use `name` as this member once `IS`
    /// has narrowed it, and the text the expression prints.
    fn narrowed(self, name: &str) -> (String, String, &'static str) {
        match self {
            Self::Int32 | Self::Int64 => (String::new(), format!("{name} + 1"), "6"),
            Self::Float32 | Self::Float64 => (String::new(), format!("{name} + 0.5"), "3.0"),
            Self::Boolean => (String::new(), format!("NOT {name}"), "FALSE"),
            Self::Str => (String::new(), format!("{name} + \"!\""), "s!"),
            Self::Null => (String::new(), name.to_owned(), "NULL"),
            Self::Na => (String::new(), name.to_owned(), "NA"),
            Self::Eof => (String::new(), name.to_owned(), "EOF"),
            Self::Error => (String::new(), format!("{name}.Code"), "1"),
            Self::Object => (
                format!("        LET bound_{name} AS Box = {name}\n"),
                "\"bound\"".to_owned(),
                "bound",
            ),
            Self::Vector => (String::new(), format!("{name}[1]"), "2"),
        }
    }

    fn language_type(self) -> Type {
        match self {
            Self::Int32 => Type::Integer(IntegerType::Int32),
            Self::Int64 => Type::Integer(IntegerType::Int64),
            Self::Float32 => Type::Float(FloatType::Float32),
            Self::Float64 => Type::Float(FloatType::Float64),
            Self::Boolean => Type::Boolean,
            Self::Str => Type::String,
            Self::Null => Type::Null,
            Self::Na => Type::NotAvailable,
            Self::Eof => Type::EndOfFile,
            Self::Error => Type::Named("Error".into()),
            Self::Object => Type::Named("Box".into()),
            Self::Vector => Type::Vector {
                element: Box::new(Type::Integer(IntegerType::Int32)),
                dimensions: vec![2],
            },
        }
    }
}

/// The first of `Error`, `NULL`, `STRING` that is not a member: the extra
/// member a widening adds.
fn widening_extra(members: &[Kind]) -> Option<Kind> {
    [Kind::Error, Kind::Null, Kind::Str]
        .into_iter()
        .find(|kind| !members.contains(kind))
}

fn spelled(members: &[Kind]) -> String {
    members
        .iter()
        .map(|kind| kind.spelling())
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// The member a numeric literal takes in `members`, by the shared rule.
fn literal_member(members: &[Kind], class: NumericClass) -> Option<Kind> {
    let types = members
        .iter()
        .map(|kind| kind.language_type())
        .collect::<Vec<_>>();
    let taken = numeric_alternative(&types, class)?;
    members
        .iter()
        .copied()
        .find(|kind| kind.language_type() == *taken)
}

/// How a value reaches `IS`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flow {
    /// `LET x AS T = value`, then `IS` in the same block.
    Linear,
    /// Passed to a parameter of type `T`.
    Parameter,
    /// Passed through a function that returns it as `T`.
    Return,
    /// Stored in a class field of type `T`.
    Field,
    /// Stored as `T`, then passed to a parameter of a wider alternative.
    Widen,
    /// A numeric literal stored as `T` (0.6.md, "Numeric literals in
    /// alternative types").
    Literal,
    /// Stored as `T`, then used as the member inside `IF x IS M THEN`.
    Narrow,
    /// Stored as `T`, then written by `PRINT` (console.md).
    Print,
    /// Stored as `T`, then compared with `=` to the value held and to another.
    Compare,
    /// Stored in a `STRUCT` field of type `T`.
    StructField,
    /// Stored in a `STATIC` field of type `T`.
    Static,
}

const FLOWS: [Flow; 11] = [
    Flow::Linear,
    Flow::Parameter,
    Flow::Return,
    Flow::Field,
    Flow::Widen,
    Flow::Literal,
    Flow::Narrow,
    Flow::Print,
    Flow::Compare,
    Flow::StructField,
    Flow::Static,
];

impl Flow {
    const fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Parameter => "parameter",
            Self::Return => "return",
            Self::Field => "field",
            Self::Widen => "widen",
            Self::Literal => "literal",
            Self::Narrow => "narrow",
            Self::Print => "print",
            Self::Compare => "compare",
            Self::StructField => "struct",
            Self::Static => "static",
        }
    }
}

/// A program for one alternative and one flow, and the text it must print.
struct Case {
    cell: String,
    source: String,
    expected: String,
}

fn is_line(label: &str, held: Kind, tested: &[Kind]) -> String {
    let answers = tested
        .iter()
        .map(|kind| if *kind == held { "TRUE" } else { "FALSE" })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{label} {answers}\n")
}

fn probe(name: &str, alternative: &str, tested: &[Kind]) -> String {
    let tests = tested
        .iter()
        .map(|kind| format!("x IS {}", kind.spelling()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "FUNCTION {name}(label AS STRING, x AS {alternative}) AS VOID\n    PRINT label, {tests}\nEND FUNCTION\n\n"
    )
}

/// Every member value, inside the narrowing tests that produce the `Error`
/// (from `ASC("")`) and `EOF` (from `INPUT()` on empty input) values.
const START: &str = "FUNCTION Start() AS VOID
    LET k_source_error AS INTEGER OR Error = ASC(\"\")
    LET k_eof AS STRING OR EOF = INPUT()
    IF k_source_error IS Error THEN
    LET k_error AS Error = k_source_error
    LET k_int32 AS INT32 = 5
    LET k_int64 AS INT64 = 5
    LET k_float32 AS FLOAT32 = 2.5
    LET k_float64 AS FLOAT64 = 2.5
    LET k_boolean AS BOOLEAN = TRUE
    LET k_string AS STRING = \"s\"
    LET k_box AS Box = NEW Box()
    LET k_vector AS INT32[2] = [1, 2]
    LET k_int32_other AS INT32 = 6
    LET k_int64_other AS INT64 = 6
    LET k_float32_other AS FLOAT32 = 3.5
    LET k_float64_other AS FLOAT64 = 3.5
    LET k_boolean_other AS BOOLEAN = FALSE
    LET k_string_other AS STRING = \"t\"
    IF k_eof IS EOF THEN
";

/// The pieces one flow contributes: declarations before `Start`, statements
/// inside it, and the lines they must print.
#[derive(Default)]
struct Parts {
    declarations: String,
    body: String,
    expected: String,
}

fn linear(members: &[Kind], alternative: &str) -> Parts {
    let mut parts = Parts::default();
    for (index, kind) in members.iter().enumerate() {
        let tests = members
            .iter()
            .map(|tested| format!("x{index} IS {}", tested.spelling()))
            .collect::<Vec<_>>()
            .join(", ");
        let label = format!("linear {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    LET x{index} AS {alternative} = {}\n    PRINT \"{label}\", {tests}",
            kind.value()
        );
        parts.expected.push_str(&is_line(&label, *kind, members));
    }
    parts
}

fn through_call(members: &[Kind], alternative: &str, flow: Flow) -> Parts {
    let mut parts = Parts {
        declarations: probe("Probe", alternative, members),
        ..Parts::default()
    };
    if flow == Flow::Return {
        let _ = write!(
            parts.declarations,
            "FUNCTION Ret(x AS {alternative}) AS {alternative}\n    RETURN x\nEND FUNCTION\n\n"
        );
    }
    for kind in members {
        let label = format!("{} {}", flow.name(), kind.spelling());
        let argument = if flow == Flow::Return {
            format!("Ret({})", kind.value())
        } else {
            kind.value().to_owned()
        };
        let _ = writeln!(parts.body, "    Probe(\"{label}\", {argument})");
        parts.expected.push_str(&is_line(&label, *kind, members));
    }
    parts
}

fn field(members: &[Kind], alternative: &str) -> Option<Parts> {
    let initializer = members.iter().find_map(|kind| kind.field_initializer())?;
    let mut parts = Parts {
        declarations: probe("Probe", alternative, members),
        body: "    LET holder AS Holder = NEW Holder()\n".to_owned(),
        ..Parts::default()
    };
    let _ = write!(
        parts.declarations,
        "CLASS Holder\n    PUBLIC slot AS {alternative} = {initializer}\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\n\n"
    );
    for kind in members {
        let label = format!("field {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    holder.slot = {}\n    Probe(\"{label}\", holder.slot)",
            kind.value()
        );
        parts.expected.push_str(&is_line(&label, *kind, members));
    }
    Some(parts)
}

fn widen(members: &[Kind], alternative: &str) -> Option<Parts> {
    let mut wider = members.to_vec();
    wider.push(widening_extra(members)?);
    let mut parts = Parts {
        declarations: probe("ProbeWide", &spelled(&wider), &wider),
        ..Parts::default()
    };
    for (index, kind) in members.iter().enumerate() {
        let label = format!("widen {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    LET x{index} AS {alternative} = {}\n    ProbeWide(\"{label}\", x{index})",
            kind.value()
        );
        parts.expected.push_str(&is_line(&label, *kind, &wider));
    }
    Some(parts)
}

fn literal(members: &[Kind], alternative: &str) -> Option<Parts> {
    let mut parts = Parts {
        declarations: probe("Probe", alternative, members),
        ..Parts::default()
    };
    for (class, literal, name) in [
        (NumericClass::Integer, "9", "integer"),
        (NumericClass::Float, "2.5", "float"),
    ] {
        if let Some(taken) = literal_member(members, class) {
            let label = format!("literal {literal}");
            let _ = writeln!(
                parts.body,
                "    LET {name} AS {alternative} = {literal}\n    Probe(\"{label}\", {name})"
            );
            parts.expected.push_str(&is_line(&label, taken, members));
        }
    }
    (!parts.expected.is_empty()).then_some(parts)
}

fn narrow(members: &[Kind], alternative: &str) -> Parts {
    let mut parts = Parts::default();
    for (index, kind) in members.iter().enumerate() {
        let name = format!("x{index}");
        let (prelude, expression, printed) = kind.narrowed(&name);
        let label = format!("narrow {}", kind.spelling());
        let _ = write!(
            parts.body,
            "    LET {name} AS {alternative} = {}\n    IF {name} IS {} THEN\n{prelude}        PRINT \"{label}\", {expression}\n    END IF\n",
            kind.value(),
            kind.spelling()
        );
        let _ = writeln!(parts.expected, "{label} {printed}");
    }
    parts
}

/// Only when the spec defines the text of every member (see `printed`).
fn print(members: &[Kind], alternative: &str) -> Option<Parts> {
    let mut parts = Parts::default();
    for (index, kind) in members.iter().enumerate() {
        let printed = kind.printed()?;
        let label = format!("print {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    LET x{index} AS {alternative} = {}\n    PRINT \"{label}\", x{index}",
            kind.value()
        );
        let _ = writeln!(parts.expected, "{label} {printed}");
    }
    Some(parts)
}

fn compare(members: &[Kind], alternative: &str) -> Option<Parts> {
    let mut parts = Parts::default();
    for (index, kind) in members.iter().enumerate() {
        let Some(other) = kind.other_value() else {
            continue;
        };
        let label = format!("compare {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    LET x{index} AS {alternative} = {value}\n    PRINT \"{label}\", x{index} = {value}, x{index} = {other}",
            value = kind.value()
        );
        let _ = writeln!(parts.expected, "{label} TRUE FALSE");
    }
    (!parts.expected.is_empty()).then_some(parts)
}

fn struct_field(members: &[Kind], alternative: &str) -> Option<Parts> {
    let initializer = members.iter().find_map(|kind| kind.field_initializer())?;
    let mut parts = Parts {
        declarations: probe("Probe", alternative, members),
        body: "    LET pair AS Pair\n".to_owned(),
        ..Parts::default()
    };
    let _ = write!(
        parts.declarations,
        "STRUCT Pair\n    slot AS {alternative} = {initializer}\nEND STRUCT\n\n"
    );
    for kind in members {
        let label = format!("struct {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    pair.slot = {}\n    Probe(\"{label}\", pair.slot)",
            kind.value()
        );
        parts.expected.push_str(&is_line(&label, *kind, members));
    }
    Some(parts)
}

fn static_field(members: &[Kind], alternative: &str) -> Option<Parts> {
    let initializer = members.iter().find_map(|kind| kind.field_initializer())?;
    let mut parts = Parts {
        declarations: probe("Probe", alternative, members),
        ..Parts::default()
    };
    let _ = write!(
        parts.declarations,
        "CLASS Counter\n    PUBLIC STATIC shared AS {alternative} = {initializer}\nEND CLASS\n\n"
    );
    for kind in members {
        let label = format!("static {}", kind.spelling());
        let _ = writeln!(
            parts.body,
            "    Counter.shared = {}\n    Probe(\"{label}\", Counter.shared)",
            kind.value()
        );
        parts.expected.push_str(&is_line(&label, *kind, members));
    }
    Some(parts)
}

/// The case for `members` through `flow`, or `None` when the flow does not
/// apply (no field initializer, no wider alternative, no literal class).
fn case(members: &[Kind], flow: Flow) -> Option<Case> {
    let alternative = spelled(members);
    let parts = match flow {
        Flow::Linear => linear(members, &alternative),
        Flow::Parameter | Flow::Return => through_call(members, &alternative, flow),
        Flow::Field => field(members, &alternative)?,
        Flow::Widen => widen(members, &alternative)?,
        Flow::Literal => literal(members, &alternative)?,
        Flow::Narrow => narrow(members, &alternative),
        Flow::Print => print(members, &alternative)?,
        Flow::Compare => compare(members, &alternative)?,
        Flow::StructField => struct_field(members, &alternative)?,
        Flow::Static => static_field(members, &alternative)?,
    };
    let source = format!(
        "CLASS Box\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\n\n{}{START}{}    END IF\n    END IF\nEND FUNCTION\n",
        parts.declarations, parts.body
    );
    Some(Case {
        cell: format!("{alternative} / {}", flow.name()),
        source,
        expected: parts.expected,
    })
}

/// The alternatives of the matrix: every pair of kinds, and triples that
/// mix a layout `bn_rt` already uses with members it does not.
fn alternatives() -> Vec<Vec<Kind>> {
    let mut all = Vec::new();
    for (index, first) in KINDS.iter().enumerate() {
        for second in &KINDS[index + 1..] {
            all.push(vec![*first, *second]);
        }
    }
    all.extend([
        vec![Kind::Int32, Kind::Null, Kind::Error],
        vec![Kind::Str, Kind::Null, Kind::Eof],
        vec![Kind::Float64, Kind::Na, Kind::Error],
        vec![Kind::Object, Kind::Null, Kind::Error],
        vec![Kind::Int32, Kind::Str, Kind::Boolean],
        vec![Kind::Float32, Kind::Float64, Kind::Error],
        vec![Kind::Int32, Kind::Int64, Kind::Null],
        vec![Kind::Vector, Kind::Null],
        vec![Kind::Vector, Kind::Error],
        vec![Kind::Vector, Kind::Str],
        vec![Kind::Int32, Kind::Vector, Kind::Null],
    ]);
    all
}

/// Whether one backend printed `expected`; the error says how it did not.
fn outcome(output: &support::ProcessOutput, expected: &str) -> Result<(), String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout == expected {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let diagnostic = stderr
        .lines()
        .find_map(|line| line.split_once("error[").map(|(_, code)| code))
        .and_then(|code| code.split_once(']'))
        .map(|(code, _)| code);
    if let Some(code) = diagnostic {
        return Err(code.to_owned());
    }
    Err("WRONG".to_owned())
}

/// Cells (`alternative / flow`) the native backend does not run as the
/// language defines (the Sprint 6 phase C work), each with what it does
/// instead: a diagnostic code, or `WRONG` for a wrong result. Remove a cell
/// when it passes.
const KNOWN_BNC_FAILURES: &[(&str, &str)] = &[];

/// Cells the interpreter does not run as the language defines. All ten are
/// one defect outside alternatives: an object stored in a `STRUCT` field
/// (`pair.slot = box`) is reported `USE_AFTER_RELEASE` at scope exit, because
/// `SetField` skips the ownership protocol (bucket typed-llvm-emitter,
/// Sprint 8).
const KNOWN_BNI_FAILURES: &[(&str, &str)] = &[];

fn run_case(case: &Case, directory: &TestDir, index: usize) -> [Result<(), String>; 2] {
    let timeout = Duration::from_secs(60);
    let path = directory.join(format!("case{index}.bn"));
    std::fs::write(&path, &case.source).expect("write case");
    let mut interpreted = run(bni().arg("run").arg(&path), None, timeout).expect("run bni");
    let interpreted_result = outcome(&interpreted, &case.expected);
    interpreted.accept_expected_status();
    let artifact = directory.join(format!("case{index}{}", std::env::consts::EXE_SUFFIX));
    let mut built = run(bnc().arg(&path).arg("-o").arg(&artifact), None, timeout).expect("run bnc");
    let compiled_result = if built.status.success() {
        let mut compiled = run(&mut std::process::Command::new(&artifact), None, timeout)
            .expect("run compiled case");
        let result = outcome(&compiled, &case.expected);
        compiled.accept_expected_status();
        result
    } else {
        let result = outcome(&built, &case.expected);
        built.accept_expected_status();
        result
    };
    [interpreted_result, compiled_result]
}

#[test]
fn alternatives_match_the_language_rule_on_both_backends() {
    let directory = TestDir::new("alternatives-matrix").expect("create test directory");
    // `BN_MATRIX_CELL=<text>` runs only the cells whose name contains it and
    // prints their source, to study one cell by hand.
    let only = std::env::var("BN_MATRIX_CELL").ok();
    let cases = alternatives()
        .iter()
        .flat_map(|members| FLOWS.iter().filter_map(|flow| case(members, *flow)))
        .filter(|case| {
            only.as_ref()
                .is_none_or(|only| case.cell.contains(only.as_str()))
        })
        .inspect(|case| {
            if only.is_some() {
                eprintln!(
                    "== {}\n{}-- expected\n{}",
                    case.cell, case.source, case.expected
                );
            }
        })
        .collect::<Vec<_>>();
    // Each case is two independent processes; run them on every core.
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(case) = cases.get(index) else {
                            return done;
                        };
                        done.push((index, run_case(case, &directory, index)));
                    }
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("matrix worker"))
            .collect::<Vec<_>>()
    });
    results.sort_by_key(|(index, _)| *index);
    let mut report = Vec::new();
    // Every failing cell, so a changed list can be pasted from the output.
    let mut failing = Vec::new();
    for (index, outcomes) in results {
        let case = &cases[index];
        for ((backend, known), result) in [("bni", KNOWN_BNI_FAILURES), ("bnc", KNOWN_BNC_FAILURES)]
            .into_iter()
            .zip(outcomes)
        {
            if let Err(reason) = &result {
                failing.push(format!("{backend}    (\"{}\", \"{reason}\"),", case.cell));
            }
            let listed = known.iter().find(|(cell, _)| *cell == case.cell);
            match (result, listed) {
                (Ok(()), Some(_)) => report.push(format!(
                    "{backend} `{}` now passes: remove it from its known-failure list",
                    case.cell
                )),
                (Err(reason), None) => {
                    report.push(format!("{backend} `{}` fails: {reason}", case.cell));
                }
                (Err(reason), Some((_, listed_reason))) if reason != *listed_reason => {
                    report.push(format!(
                        "{backend} `{}` fails differently: {reason}, listed {listed_reason}",
                        case.cell
                    ));
                }
                _ => {}
            }
        }
    }
    assert!(
        report.is_empty(),
        "{}\n\nfailing cells:\n{}",
        report.join("\n"),
        failing.join("\n")
    );
}
