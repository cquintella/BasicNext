#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn qualified_type_name(atom: &crate::ast::TypeAtom) -> String {
    let mut name = atom.name.clone();
    let mut parts = atom.parts.iter();
    while matches!(parts.next(), Some(part) if part == "Dot") {
        let Some(part) = parts.next() else { break };
        name.push('.');
        name.push_str(part);
    }
    name
}

pub(crate) fn pointer_element_type(parts: &[String]) -> Type {
    let Some(start) = parts
        .iter()
        .position(|part| part == "TO")
        .map(|index| index + 1)
    else {
        return Type::Unknown;
    };
    let end = parts[start..]
        .iter()
        .position(|part| part == "LeftBracket")
        .map_or(parts.len(), |index| start + index);
    let element = &parts[start..end];
    let Some(first) = element.first() else {
        return Type::Unknown;
    };
    if element.len() == 1 {
        return type_from_name(first);
    }
    let mut name = first.clone();
    let mut suffix = element[1..].chunks_exact(2);
    for pair in &mut suffix {
        if pair[0] != "Dot" {
            return Type::Unknown;
        }
        name.push('.');
        name.push_str(&pair[1]);
    }
    if !suffix.remainder().is_empty() {
        return Type::Unknown;
    }
    type_from_name(&name)
}

pub(crate) fn pointer_length(parts: &[String]) -> PointerLength {
    let Some(open) = parts.iter().position(|part| part == "LeftBracket") else {
        return PointerLength::One;
    };
    parts
        .get(open + 1)
        .filter(|part| part.as_str() != "RightBracket")
        .and_then(|part| parse_integer(part))
        .and_then(|part| u64::try_from(part).ok())
        .map_or(PointerLength::Dynamic, PointerLength::Fixed)
}
