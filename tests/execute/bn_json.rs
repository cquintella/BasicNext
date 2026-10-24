// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
#[test]
fn bnjson_provider_reports_gate_bound_unavailability() {
    let source = r#"IMPORT BNJson AS Json
FUNCTION Start() AS VOID
LET value AS Json.Json OR Error = Json.Json.Parse("{\"ok\":true}")
IF value IS Error THEN
PRINT "error"
ELSE
PRINT Json.Json.Stringify(value)
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNJson provider");
    assert_eq!(output, "{\"ok\":true}\n");
}
