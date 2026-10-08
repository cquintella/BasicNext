// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared nominal class and interface hierarchy model.
//!
//! Provides a single source of truth for nominal subtyping (`is_upcast`)
//! and class inheritance ancestry (`ancestors`).

use std::collections::{BTreeMap, BTreeSet};

/// Nominal inheritance and interface implementation model.
///
/// Qualified names are stored in canonical form (`"Dog"`, `"#0.Dog"`).
/// Sorted maps guarantee deterministic emission and iteration order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassModel {
    /// Maps each class to its direct declared base class.
    pub bases: BTreeMap<String, String>,
    /// Maps each class to its directly declared implemented interfaces.
    pub interfaces: BTreeMap<String, BTreeSet<String>>,
}

impl ClassModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `class_name` extends `base_name`.
    pub fn add_base(&mut self, class_name: impl Into<String>, base_name: impl Into<String>) {
        self.bases.insert(class_name.into(), base_name.into());
    }

    /// Records that `class_name` implements `interface_name`.
    pub fn add_interface(
        &mut self,
        class_name: impl Into<String>,
        interface_name: impl Into<String>,
    ) {
        self.interfaces
            .entry(class_name.into())
            .or_default()
            .insert(interface_name.into());
    }

    /// Returns the inheritance chain starting from `class_name` up through all base classes.
    ///
    /// Cycles are guarded by a visited set (though frontends reject `INHERITANCE_CYCLE`).
    #[must_use]
    pub fn ancestors<'a>(&'a self, class_name: &'a str) -> Vec<&'a str> {
        let mut result = Vec::new();
        let mut current = Some(class_name);
        let mut visited = BTreeSet::new();

        while let Some(cls) = current {
            if !visited.insert(cls) {
                break;
            }
            result.push(cls);
            current = self.bases.get(cls).map(String::as_str);
        }

        result
    }

    /// Determines whether a value of nominal type `source` can be upcast to `target`.
    ///
    /// An upcast succeeds if:
    /// 1. `source == target`
    /// 2. `target` is an ancestor class of `source`
    /// 3. `target` is an interface implemented by `source` or by any ancestor class of `source`
    #[must_use]
    pub fn is_upcast(&self, source: &str, target: &str) -> bool {
        if source == target {
            return true;
        }

        for ancestor in self.ancestors(source) {
            if ancestor == target {
                return true;
            }
            if let Some(ifaces) = self.interfaces.get(ancestor)
                && ifaces.contains(target)
            {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::ClassModel;

    #[test]
    fn two_level_inheritance_chain_and_ancestors() {
        let mut model = ClassModel::new();
        model.add_base("Mid", "Base");
        model.add_base("Sub", "Mid");

        assert_eq!(model.ancestors("Sub"), vec!["Sub", "Mid", "Base"]);
        assert_eq!(model.ancestors("Mid"), vec!["Mid", "Base"]);
        assert_eq!(model.ancestors("Base"), vec!["Base"]);
        assert_eq!(model.ancestors("Unrelated"), vec!["Unrelated"]);

        assert!(model.is_upcast("Sub", "Mid"));
        assert!(model.is_upcast("Sub", "Base"));
        assert!(model.is_upcast("Sub", "Sub"));
        assert!(!model.is_upcast("Base", "Sub"));
        assert!(!model.is_upcast("Mid", "Sub"));
    }

    #[test]
    fn interface_on_base_and_interface_on_class() {
        let mut model = ClassModel::new();
        model.add_base("Mid", "Base");
        model.add_base("Sub", "Mid");
        model.add_interface("Base", "Shape");
        model.add_interface("Sub", "Drawable");

        // Interface on base is inherited by Sub
        assert!(model.is_upcast("Base", "Shape"));
        assert!(model.is_upcast("Mid", "Shape"));
        assert!(model.is_upcast("Sub", "Shape"));

        // Interface on Sub is not implemented by Base or Mid
        assert!(model.is_upcast("Sub", "Drawable"));
        assert!(!model.is_upcast("Mid", "Drawable"));
        assert!(!model.is_upcast("Base", "Drawable"));
    }

    #[test]
    fn unrelated_class_and_cycle_guard() {
        let mut model = ClassModel::new();
        model.add_base("A", "B");
        model.add_base("B", "A"); // Cycle guard test

        assert_eq!(model.ancestors("A"), vec!["A", "B"]);
        assert!(model.is_upcast("A", "B"));
        assert!(!model.is_upcast("A", "Unrelated"));
    }

    #[test]
    fn classes_from_other_modules_qualified() {
        let mut model = ClassModel::new();
        model.add_base("Dog", "Animal");
        model.add_base("#0.Dog", "#0.Animal");
        model.add_interface("#0.Dog", "#0.Pet");

        assert!(model.is_upcast("Dog", "Animal"));
        assert!(!model.is_upcast("Dog", "#0.Animal"));
        assert!(model.is_upcast("#0.Dog", "#0.Animal"));
        assert!(model.is_upcast("#0.Dog", "#0.Pet"));
        assert!(!model.is_upcast("Dog", "#0.Pet"));
    }
}
