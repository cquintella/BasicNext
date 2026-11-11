// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNWeb` contract and its module name the same functions and methods. Every
//! function `language/0.6/bnweb.md` writes in call form — a backticked
//! name followed by a list of parameter names, as in `ClientReply.Status()` or
//! `Client.Get(url, maximum)` — is declared in `modules/bn/BNWeb.bn`.

use std::collections::BTreeSet;
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
