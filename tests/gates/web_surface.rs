// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNWeb` contract and its module name the same functions and methods. Every
//! function `language/0.6/bnweb.md` writes in call form — a backticked
//! name followed by a list of parameter names, as in `ClientReply.Status()` or
//! `Client.Get(url, maximum)` — is declared in `modules/bn/BNWeb.bn`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

const CONTRACT: &str = "language/0.6/bnweb.md";
const MODULE: &str = "modules/bn/BNWeb.bn";

/// The words that declare a name in a module, each followed by that name.
const DECLARATIONS: [&str; 4] = [
    "EXPORT FUNCTION ",
    "EXPORT STRUCT ",
    "EXPORT CLASS ",
    "FUNCTION ",
];

fn is_identifier(word: &str) -> bool {
    let mut characters = word.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// One parameter of a call form: a name, optionally `AS` a type.
fn is_parameter(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let Some(name) = words.next() else {
        return false;
    };
    match (words.next(), words.next()) {
        (None, None) | (Some("AS"), Some(_)) => is_identifier(name),
        _ => false,
    }
}

/// The last name of a dotted call target (`Client.Get` → `Get`), when the
/// span is a call form.
fn function_of(span: &str) -> Option<&str> {
    let (target, rest) = span.split_once('(')?;
    let (arguments, _) = rest.split_once(')')?;
    let target = target.strip_prefix('.').unwrap_or(target);
    if !target.split('.').all(is_identifier) {
        return None;
    }
    let parameters_are_names =
        arguments.trim().is_empty() || arguments.split(',').all(is_parameter);
    parameters_are_names.then(|| target.rsplit('.').next().unwrap_or(target))
}

/// Every function the contract text writes in call form.
pub(crate) fn contract_functions(contract: &str) -> BTreeSet<String> {
    contract
        .split('`')
        .skip(1)
        .step_by(2)
        .filter_map(function_of)
        .map(str::to_owned)
        .collect()
}

/// Every function, structure and class the module declares.
pub(crate) fn module_names(module: &str) -> BTreeSet<String> {
    module
        .lines()
        .map(str::trim_start)
        .filter_map(|line| {
            let line = line.strip_prefix("PUBLIC ").unwrap_or(line);
            let line = line.strip_prefix("STATIC ").unwrap_or(line);
            DECLARATIONS
                .iter()
                .find_map(|word| line.strip_prefix(word))
                .and_then(|rest| rest.split(['(', ' ']).next())
                .filter(|name| is_identifier(name))
                .map(str::to_owned)
        })
        .collect()
}

fn missing(contract: &str, module: &str) -> Vec<String> {
    let declared = module_names(module);
    contract_functions(contract)
        .into_iter()
        .filter(|name| !declared.contains(name))
        .collect()
}

#[test]
fn every_function_the_bnweb_contract_names_is_declared() {
    let contract = fs::read_to_string(CONTRACT).expect("read the BNWeb contract");
    let module = fs::read_to_string(MODULE).expect("read the BNWeb module");
    assert_eq!(missing(&contract, &module), Vec::<String>::new());
}

#[test]
fn a_planted_name_fails_in_web_surface() {
    let module = fs::read_to_string(MODULE).expect("read the BNWeb module");
    let planted = "`Client.PlantedMethod(data AS STRING)`";
    assert_eq!(missing(planted, &module), ["PlantedMethod"]);
}

// ---------------------------------------------------------------------------
// Served surface: every member `modules/bn/BNWeb.bn` declares is dispatched by
// the interpreter provider and lowered by the native name map, or is listed in
// `tests/web_surface.allow` with its reason. The list only shrinks: a listed
// member that a backend now serves fails the gate until its line is removed.

/// The interpreter provider of `BNWeb`.
const BNI_PROVIDER: &str = "libraries/bn_lib_web/src";
/// The native name map of `BNWeb` (`"Class.Member" => Some(op)`).
const BNC_NAME_MAP: &str = "crates/bn_llvm/src/llvm/web_helpers.rs";
/// Accepted gaps: `Class.Member<TAB>backend<TAB>reason`.
const ALLOW: &str = "tests/web_surface.allow";
/// The two backends, as the allow file names them.
const BACKENDS: [&str; 2] = ["bni", "bnc"];

/// `Class.Member` for every `FUNCTION` of every `EXPORT CLASS` of a module,
/// `CONSTRUCTOR` included.
pub(crate) fn declared_members(module: &str) -> BTreeSet<String> {
    let mut class: Option<&str> = None;
    let mut members = BTreeSet::new();
    for line in module.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("EXPORT CLASS ") {
            class = rest.split_whitespace().next();
            continue;
        }
        if line == "END CLASS" {
            class = None;
            continue;
        }
        let Some(owner) = class else {
            continue;
        };
        let line = line.strip_prefix("PUBLIC ").unwrap_or(line);
        let line = line.strip_prefix("STATIC ").unwrap_or(line);
        if let Some(name) = line
            .strip_prefix("FUNCTION ")
            .and_then(|rest| rest.split('(').next())
            .filter(|name| is_identifier(name))
        {
            members.insert(format!("{owner}.{name}"));
        }
    }
    members
}

/// The classes of a set of `Class.Member` names.
fn classes_of(members: &BTreeSet<String>) -> BTreeSet<String> {
    members
        .iter()
        .filter_map(|member| member.split_once('.').map(|(class, _)| class.to_owned()))
        .collect()
}

/// `ClientReply` → `client_reply`.
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// Where a class's dispatch starts in a provider source: the class test of
/// the member name (`".Client."`) and the class's call function
/// (`fn web_client_reply_call(`).
fn class_markers(text: &str, classes: &BTreeSet<String>) -> Vec<(usize, String)> {
    let mut markers = Vec::new();
    for class in classes {
        for needle in [
            format!("\".{class}.\""),
            format!("fn web_{}_call(", snake_case(class)),
        ] {
            markers.extend(
                text.match_indices(needle.as_str())
                    .map(|(at, _)| (at, class.clone())),
            );
        }
    }
    markers.sort();
    markers
}

/// The member names a dispatch region matches: a quoted capitalised name in a
/// pattern position (`"Get" =>`, `"Get" |`, `| "Get"`, `method == "Get"`).
fn dispatched_names(region: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let pieces = region.split('"').collect::<Vec<_>>();
    // Odd pieces are the insides of string literals.
    for index in (1..pieces.len()).step_by(2) {
        let name = pieces[index];
        if !is_identifier(name) || !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let before = pieces[index - 1].trim_end();
        let after = pieces.get(index + 1).map_or("", |piece| piece.trim_start());
        if after.starts_with("=>")
            || after.starts_with('|')
            || before.ends_with("==")
            || before.ends_with('|')
        {
            names.insert(name.to_owned());
        }
    }
    names
}

/// The provider's generic constructor path: a `NEW` of a class with no
/// constructor of its own, and the arm that lets one through.
const LIFECYCLE_PATH: &str = "is_lifecycle_stub(";

/// The dispatch text of each class: from a group of markers to the next
/// group. Markers joined by `||` on one test (`".HeaderValues.") ||
/// name.contains(".QueryValues.")`) share the block that follows them.
fn class_regions(sources: &[String], classes: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut regions: BTreeMap<String, String> = BTreeMap::new();
    for text in sources {
        let markers = class_markers(text, classes);
        let mut index = 0;
        while index < markers.len() {
            let mut group = vec![markers[index].1.clone()];
            let mut last = markers[index].0;
            while let Some((next, class)) = markers.get(index + 1) {
                if text.get(last..*next).is_some_and(|gap| gap.contains('{')) {
                    break;
                }
                group.push(class.clone());
                last = *next;
                index += 1;
            }
            let end = markers.get(index + 1).map_or(text.len(), |(at, _)| *at);
            let region = text.get(markers[index].0..end).unwrap_or_default();
            for class in group {
                regions.entry(class).or_default().push_str(region);
            }
            index += 1;
        }
    }
    regions
}

/// `Class.Member` the interpreter provider dispatches. A `CONSTRUCTOR` is
/// served by its own arm, by the lifecycle arm of its class, or — for a class
/// with no dispatch of its own — by the provider's lifecycle fallback.
pub(crate) fn bni_members(sources: &[String], classes: &BTreeSet<String>) -> BTreeSet<String> {
    let regions = class_regions(sources, classes);
    let fallback = sources.iter().any(|text| text.contains(LIFECYCLE_PATH));
    let mut served = BTreeSet::new();
    for class in classes {
        let constructor = match regions.get(class) {
            Some(region) => {
                served.extend(
                    dispatched_names(region)
                        .into_iter()
                        .map(|name| format!("{class}.{name}")),
                );
                region.contains(LIFECYCLE_PATH)
            }
            None => fallback,
        };
        if constructor {
            served.insert(format!("{class}.CONSTRUCTOR"));
        }
    }
    served
}

/// `Class.Member` the native name map lowers.
pub(crate) fn bnc_members(map: &str) -> BTreeSet<String> {
    let pieces = map.split('"').collect::<Vec<_>>();
    (1..pieces.len())
        .step_by(2)
        .filter(|&index| {
            pieces
                .get(index + 1)
                .is_some_and(|after| after.trim_start().starts_with("=> Some("))
        })
        .map(|index| pieces[index])
        .filter(|name| {
            name.split_once('.')
                .is_some_and(|(class, member)| is_identifier(class) && is_identifier(member))
        })
        .map(str::to_owned)
        .collect()
}

/// The problems of a served surface; empty when the gate passes.
pub(crate) fn surface_report(
    declared: &BTreeSet<String>,
    served: [&BTreeSet<String>; 2],
    allow: &str,
) -> Vec<String> {
    let mut report = Vec::new();
    let mut listed = BTreeSet::new();
    for line in allow.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        let [member, backend, reason] = fields.as_slice() else {
            report.push(format!(
                "{ALLOW}: `{line}` is not member<TAB>backend<TAB>reason"
            ));
            continue;
        };
        let Some(side) = BACKENDS.iter().position(|name| name == backend) else {
            report.push(format!("{ALLOW}: `{backend}` is not one of {BACKENDS:?}"));
            continue;
        };
        if reason.trim().is_empty() {
            report.push(format!("{ALLOW}: {member} {backend} gives no reason"));
        }
        if !listed.insert(((*member).to_owned(), side)) {
            report.push(format!("{ALLOW}: {member} {backend} is listed twice"));
        }
        if !declared.contains(*member) {
            report.push(format!(
                "{member} {backend}: listed but not declared; remove its line"
            ));
        } else if served[side].contains(*member) {
            report.push(format!(
                "{member} {backend}: listed but served; remove its line"
            ));
        }
    }
    for member in declared {
        for (side, backend) in BACKENDS.iter().enumerate() {
            if !served[side].contains(member) && !listed.contains(&(member.clone(), side)) {
                report.push(format!("{member} {backend}: declared but not served"));
            }
        }
    }
    report
}

fn provider_sources() -> Vec<String> {
    let mut paths = fs::read_dir(BNI_PROVIDER)
        .expect("read the BNWeb provider")
        .map(|entry| entry.expect("provider entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .iter()
        .map(|path| fs::read_to_string(path).expect("read a provider source"))
        .collect()
}

#[test]
fn every_declared_bnweb_member_is_served_by_both_backends() {
    let module = fs::read_to_string(MODULE).expect("read the BNWeb module");
    let declared = declared_members(&module);
    let bni = bni_members(&provider_sources(), &classes_of(&declared));
    let bnc = bnc_members(&fs::read_to_string(BNC_NAME_MAP).expect("read the native name map"));
    let allow = fs::read_to_string(ALLOW).expect("read the BNWeb surface allow file");
    let report = surface_report(&declared, [&bni, &bnc], &allow);
    assert!(report.is_empty(), "{}", report.join("\n"));
}

#[test]
fn a_planted_declaration_and_a_served_allow_entry_fail() {
    let module = "EXPORT CLASS Client\n    PUBLIC FUNCTION Get(url AS STRING) AS INTEGER\n    END FUNCTION\n    PUBLIC STATIC FUNCTION Planted() AS INTEGER\n    END FUNCTION\nEND CLASS\n";
    let declared = declared_members(module);
    let provider = vec![
        "if name.contains(\".Client.\") {\n    match method {\n        \"Get\" => {}\n        _ => {}\n    }\n}\n"
            .to_owned(),
    ];
    let bni = bni_members(&provider, &classes_of(&declared));
    let bnc = bnc_members("match rest {\n    \"Client.Get\" => Some(\"client_get\"),\n}\n");
    assert_eq!(
        surface_report(&declared, [&bni, &bnc], ""),
        [
            "Client.Planted bni: declared but not served",
            "Client.Planted bnc: declared but not served",
        ],
        "a declaration no backend serves is reported once per backend"
    );
    let allow =
        "Client.Planted\tbni\tplanted\nClient.Planted\tbnc\tplanted\nClient.Get\tbnc\tstale\n";
    assert_eq!(
        surface_report(&declared, [&bni, &bnc], allow),
        ["Client.Get bnc: listed but served; remove its line"],
        "the allow file only shrinks"
    );
}
