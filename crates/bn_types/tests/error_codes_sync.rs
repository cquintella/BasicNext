// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Verifies that `bn_types::error_codes::sqlite::ALL` matches the constants
//! exported in `modules/bn/BNSqlite.bn`.

use bn_types::error_codes::sqlite;

#[test]
fn sqlite_error_codes_match_module_exports() {
    let module = include_str!("../../../modules/bn/BNSqlite.bn");

    // Every constant in ALL must appear as an EXPORT CONST in BNSqlite.bn
    for &(name, value) in sqlite::ALL {
        let expected_line = format!("EXPORT CONST {name} AS INTEGER = {value}");
        assert!(
            module.contains(&expected_line),
            "modules/bn/BNSqlite.bn lacks `{expected_line}`"
        );
    }

    // Verify all exported constants in BNSqlite.bn are in ALL
    let mut exported_count = 0;
    for line in module.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("EXPORT CONST ") {
            exported_count += 1;
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            // EXPORT CONST <NAME> AS INTEGER = <VALUE>
            assert!(parts.len() >= 7, "Malformed EXPORT CONST line: {trimmed}");
            let const_name = parts[2];
            let const_val: i32 = parts[6]
                .parse()
                .unwrap_or_else(|_| panic!("Failed to parse integer value in: {trimmed}"));

            let found = sqlite::ALL
                .iter()
                .find(|&&(name, val)| name == const_name && val == const_val);
            assert!(
                found.is_some(),
                "Exported constant {const_name} = {const_val} in BNSqlite.bn not found in sqlite::ALL"
            );
        }
    }

    assert_eq!(
        exported_count,
        sqlite::ALL.len(),
        "Count of exported constants in BNSqlite.bn does not match sqlite::ALL.len()"
    );
}
