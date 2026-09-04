#![allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn debug_variables(
    symbols: &HashMap<SymbolId, Value>,
    values: &HashMap<ValueId, Value>,
) -> Vec<DebugVariable> {
    let mut variables = symbols
        .iter()
        .map(|(symbol, value)| DebugVariable {
            name: format!("symbol#{}", symbol.value()),
            value: format!("{value:?}"),
        })
        .collect::<Vec<_>>();
    variables.extend(values.iter().map(|(value_id, value)| DebugVariable {
        name: format!("value#{}", value_id.value()),
        value: format!("{value:?}"),
    }));
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    variables
}

pub(super) fn host_random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let seed = u64::try_from(nanos).unwrap_or(u64::MAX) ^ u64::from(std::process::id());
    seed.max(1)
}
