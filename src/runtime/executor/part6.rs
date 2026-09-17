#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn standard_provider(name: &str, providers: &HashSet<ModuleId>) -> bool {
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

}
