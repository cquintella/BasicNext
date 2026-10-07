// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Ownership conformance matrix (bucket typed-llvm-emitter, Sprint 8 phase
//! A): every way of holding an object (local, parameter, return, field from
//! outside and from a method, `STRUCT` field, vector element, `STATIC`,
//! `WEAK`, alternatives) crossed with every way of letting it go (end of
//! scope, reassignment, `RELEASE`, end of the owner). Each cell is one
//! program whose `DESTRUCTOR` prints a line, so the output shows when each
//! object is freed; the expected output follows the ARC rules of 0.6.md
//! ("Memory model (ARC)", "`RELEASE`"), never one backend's output. Each
//! cell runs in its own function, so the release at its end is observed
//! before `after`, and no cell depends on the order of releases inside one
//! block (0.6.md leaves it unspecified).

mod support;

use std::time::Duration;

use support::{TestDir, bnc, bni, run};

const HEADER: &str = "CLASS Box
    PUBLIC name AS STRING = \"\"
    PUBLIC FUNCTION CONSTRUCTOR(label AS STRING)
        SELF.name = label
    END FUNCTION
    PUBLIC FUNCTION DESTRUCTOR()
        PRINT \"free\", SELF.name
    END FUNCTION
END CLASS

CLASS Holder
    PUBLIC slot AS Box OR NULL = NULL
    PUBLIC general AS Box OR STRING = \"\"
    PUBLIC STATIC shared AS Box OR NULL = NULL
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
    PUBLIC FUNCTION Put(item AS Box OR NULL) AS VOID
        SELF.slot = item
    END FUNCTION
END CLASS

STRUCT Pair
    item AS Box OR NULL = NULL
END STRUCT

FUNCTION Use(x AS Box) AS VOID
    PRINT \"in\"
END FUNCTION

FUNCTION Make(label AS STRING) AS Box
    RETURN NEW Box(label)
END FUNCTION

FUNCTION Report(x AS Box OR NULL) AS VOID
    PRINT x IS NULL
END FUNCTION

FUNCTION ReleaseParameter(x AS Box) AS VOID
    RELEASE x
    PRINT \"in\"
END FUNCTION

";

/// One cell: its name (`holding / letting go`), the body of `Case`, and the
/// exact output the rules give.
const CELLS: &[(&str, &str, &str)] = &[
    (
        "local / end of scope",
        "LET b AS Box = NEW Box(\"b\")",
        "free b\nafter\n",
    ),
    (
        "local / reassignment",
        "LET b AS Box = NEW Box(\"b1\")\n    b = NEW Box(\"b2\")\n    PRINT \"mid\"",
        "free b1\nmid\nfree b2\nafter\n",
    ),
    (
        "local / RELEASE",
        "LET b AS Box = NEW Box(\"b\")\n    RELEASE b\n    PRINT \"mid\"",
        "free b\nmid\nafter\n",
    ),
    (
        "alias / RELEASE of one alias",
        "LET a AS Box = NEW Box(\"a\")\n    LET c AS Box = a\n    RELEASE a\n    PRINT \"mid\"",
        "mid\nfree a\nafter\n",
    ),
    (
        "parameter / borrowed by the callee",
        "LET b AS Box = NEW Box(\"b\")\n    Use(b)\n    PRINT \"mid\"",
        "in\nmid\nfree b\nafter\n",
    ),
    (
        "parameter / temporary argument",
        "Use(NEW Box(\"t\"))\n    PRINT \"mid\"",
        "in\nfree t\nmid\nafter\n",
    ),
    (
        "return / kept by the caller",
        "LET b AS Box = Make(\"r\")\n    PRINT \"mid\"",
        "mid\nfree r\nafter\n",
    ),
    (
        "return / discarded",
        "Make(\"d\")\n    PRINT \"mid\"",
        "free d\nmid\nafter\n",
    ),
    (
        "field from outside / reassignment",
        "LET h AS Holder = NEW Holder()\n    h.slot = NEW Box(\"f1\")\n    h.slot = NEW Box(\"f2\")\n    PRINT \"mid\"",
        "free f1\nmid\nfree f2\nafter\n",
    ),
    (
        "field from outside / set to NULL",
        "LET h AS Holder = NEW Holder()\n    h.slot = NEW Box(\"f\")\n    h.slot = NULL\n    PRINT \"mid\"",
        "free f\nmid\nafter\n",
    ),
    (
        "field from outside / RELEASE of the owner",
        "LET h AS Holder = NEW Holder()\n    h.slot = NEW Box(\"f\")\n    RELEASE h\n    PRINT \"mid\"",
        "free f\nmid\nafter\n",
    ),
    (
        "field from outside / alias outlives the owner",
        "LET k AS Box = NEW Box(\"k\")\n    LET h AS Holder = NEW Holder()\n    h.slot = k\n    RELEASE h\n    PRINT \"mid\"",
        "mid\nfree k\nafter\n",
    ),
    (
        "field from a method / reassignment",
        "LET h AS Holder = NEW Holder()\n    h.Put(NEW Box(\"m1\"))\n    h.Put(NEW Box(\"m2\"))\n    PRINT \"mid\"",
        "free m1\nmid\nfree m2\nafter\n",
    ),
    (
        "field from a method / set to NULL",
        "LET h AS Holder = NEW Holder()\n    h.Put(NEW Box(\"m\"))\n    h.Put(NULL)\n    PRINT \"mid\"",
        "free m\nmid\nafter\n",
    ),
    (
        "STATIC / reassignment",
        "Holder.shared = NEW Box(\"s1\")\n    Holder.shared = NEW Box(\"s2\")\n    PRINT \"mid\"\n    Holder.shared = NULL",
        "free s1\nmid\nfree s2\nafter\n",
    ),
    (
        "STATIC / alias outlives the static",
        "LET k AS Box = NEW Box(\"k\")\n    Holder.shared = k\n    Holder.shared = NULL\n    PRINT \"mid\"",
        "mid\nfree k\nafter\n",
    ),
    (
        "STRUCT field / reassignment",
        "LET p AS Pair\n    p.item = NEW Box(\"p1\")\n    p.item = NEW Box(\"p2\")\n    PRINT \"mid\"",
        "free p1\nmid\nfree p2\nafter\n",
    ),
    (
        "STRUCT field / RELEASE of the struct",
        "LET p AS Pair\n    p.item = NEW Box(\"p\")\n    RELEASE p\n    PRINT \"mid\"",
        "free p\nmid\nafter\n",
    ),
    (
        "STRUCT field / copy of the struct",
        "LET p AS Pair\n    p.item = NEW Box(\"c\")\n    LET q AS Pair = p\n    RELEASE p\n    PRINT \"mid\"",
        "mid\nfree c\nafter\n",
    ),
    (
        "vector element / reassignment",
        "LET v AS Box[1] = [NEW Box(\"v0\")]\n    v[0] = NEW Box(\"v1\")\n    PRINT \"mid\"",
        "free v0\nmid\nfree v1\nafter\n",
    ),
    (
        "vector element / RELEASE of the vector",
        "LET v AS Box[1] = [NEW Box(\"v\")]\n    RELEASE v\n    PRINT \"mid\"",
        "free v\nmid\nafter\n",
    ),
    (
        "WEAK / reads NULL after the last strong",
        "LET s AS Box = NEW Box(\"w\")\n    LET w AS WEAK Box OR NULL = s\n    RELEASE s\n    PRINT w IS NULL",
        "free w\nTRUE\nafter\n",
    ),
    (
        "Class OR NULL / set to NULL",
        "LET o AS Box OR NULL = NEW Box(\"o\")\n    o = NULL\n    PRINT \"mid\"",
        "free o\nmid\nafter\n",
    ),
    (
        "Class OR Error / reassignment",
        "LET e AS Box OR Error = NEW Box(\"e1\")\n    e = NEW Box(\"e2\")\n    PRINT \"mid\"",
        "free e1\nmid\nfree e2\nafter\n",
    ),
    (
        "general alternative / reassignment",
        "LET g AS Box OR STRING = NEW Box(\"g\")\n    g = \"text\"\n    PRINT \"mid\"",
        "free g\nmid\nafter\n",
    ),
    (
        "general alternative / RELEASE",
        "LET g AS Box OR STRING = NEW Box(\"g\")\n    RELEASE g\n    PRINT \"mid\"",
        "free g\nmid\nafter\n",
    ),
    (
        "general alternative field / reassignment",
        "LET h AS Holder = NEW Holder()\n    h.general = NEW Box(\"g\")\n    h.general = \"text\"\n    PRINT \"mid\"",
        "free g\nmid\nafter\n",
    ),
    (
        "alias / end of scope of both",
        "LET a AS Box = NEW Box(\"a\")\n    LET c AS Box = a\n    PRINT \"mid\"",
        "mid\nfree a\nafter\n",
    ),
    (
        "alias / reassignment of one alias",
        "LET a AS Box = NEW Box(\"a\")\n    LET c AS Box = a\n    a = NEW Box(\"n\")\n    PRINT \"mid\"\n    RELEASE c\n    PRINT \"c gone\"\n    RELEASE a",
        "mid\nfree a\nc gone\nfree n\nafter\n",
    ),
    (
        "parameter / RELEASE in the callee",
        "LET b AS Box = NEW Box(\"b\")\n    ReleaseParameter(b)\n    PRINT \"mid\"",
        "in\nmid\nfree b\nafter\n",
    ),
    (
        "return / RELEASE of the received value",
        "LET b AS Box = Make(\"r\")\n    RELEASE b\n    PRINT \"mid\"",
        "free r\nmid\nafter\n",
    ),
    (
        "field from outside / end of scope of the owner",
        "LET h AS Holder = NEW Holder()\n    h.slot = NEW Box(\"f\")\n    PRINT \"mid\"",
        "mid\nfree f\nafter\n",
    ),
    (
        "field from a method / end of scope of the owner",
        "LET h AS Holder = NEW Holder()\n    h.Put(NEW Box(\"m\"))\n    PRINT \"mid\"",
        "mid\nfree m\nafter\n",
    ),
    (
        "field from a method / RELEASE of the owner",
        "LET h AS Holder = NEW Holder()\n    h.Put(NEW Box(\"m\"))\n    RELEASE h\n    PRINT \"mid\"",
        "free m\nmid\nafter\n",
    ),
    (
        "STRUCT field / end of scope",
        "LET p AS Pair\n    p.item = NEW Box(\"p\")\n    PRINT \"mid\"",
        "mid\nfree p\nafter\n",
    ),
    (
        "vector element / end of scope",
        "LET v AS Box[1] = [NEW Box(\"v\")]\n    PRINT \"mid\"",
        "mid\nfree v\nafter\n",
    ),
    (
        "WEAK / an object held only by a weak binding",
        "LET w AS WEAK Box OR NULL = NULL\n    w = Make(\"w\")\n    Report(w)",
        "free w\nTRUE\nafter\n",
    ),
    (
        "WEAK / last strong reassigned",
        "LET s AS Box = NEW Box(\"w\")\n    LET w AS WEAK Box OR NULL = s\n    s = NEW Box(\"n\")\n    PRINT w IS NULL",
        "free w\nTRUE\nfree n\nafter\n",
    ),
    (
        "Class OR NULL / end of scope",
        "LET o AS Box OR NULL = NEW Box(\"o\")\n    PRINT \"mid\"",
        "mid\nfree o\nafter\n",
    ),
    (
        "Class OR NULL / RELEASE",
        "LET o AS Box OR NULL = NEW Box(\"o\")\n    RELEASE o\n    PRINT \"mid\"",
        "free o\nmid\nafter\n",
    ),
    (
        "Class OR Error / end of scope",
        "LET e AS Box OR Error = NEW Box(\"e\")\n    PRINT \"mid\"",
        "mid\nfree e\nafter\n",
    ),
    (
        "general alternative / end of scope",
        "LET g AS Box OR STRING = NEW Box(\"g\")\n    PRINT \"mid\"",
        "mid\nfree g\nafter\n",
    ),
    (
        "general alternative field / end of scope of the owner",
        "LET h AS Holder = NEW Holder()\n    h.general = NEW Box(\"g\")\n    PRINT \"mid\"",
        "mid\nfree g\nafter\n",
    ),
    (
        "loop body local / end of each iteration",
        "FOR i AS INTEGER = 1 TO 2\n        LET l AS Box = NEW Box(\"l\" + (i AS STRING))\n    END FOR\n    PRINT \"mid\"",
        "free l1\nfree l2\nmid\nafter\n",
    ),
    (
        "block local alternative / end of the IF block",
        "IF TRUE THEN\n        LET o AS Box OR NULL = NEW Box(\"o\")\n    END IF\n    PRINT \"mid\"",
        "free o\nmid\nafter\n",
    ),
    (
        "two locals of one block / reverse declaration order",
        "IF TRUE THEN\n        LET first AS Box = NEW Box(\"first\")\n        LET second AS Box = NEW Box(\"second\")\n    END IF\n    PRINT \"mid\"",
        "free second\nfree first\nmid\nafter\n",
    ),
];

/// Cells a backend does not run as the rules define, each with what it does
/// instead (a diagnostic code, or `WRONG`). A ratchet: remove a cell when
/// it passes; never add one to make the test green.
const KNOWN_BNI_FAILURES: &[(&str, &str)] = &[];
const KNOWN_BNC_FAILURES: &[(&str, &str)] = &[
    ("STRUCT field / reassignment", "TARGET_UNSUPPORTED_OP"),
    (
        "STRUCT field / RELEASE of the struct",
        "TARGET_UNSUPPORTED_OP",
    ),
    ("STRUCT field / copy of the struct", "TARGET_UNSUPPORTED_OP"),
    ("STRUCT field / end of scope", "TARGET_UNSUPPORTED_OP"),
];

fn source(body: &str) -> String {
    format!(
        "{HEADER}FUNCTION Case() AS VOID\n    {body}\nEND FUNCTION\n\nFUNCTION Start() AS VOID\n    Case()\n    PRINT \"after\"\nEND FUNCTION\n"
    )
}

fn outcome(output: &support::ProcessOutput, expected: &str) -> Result<(), String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout == expected {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = stderr
        .lines()
        .find_map(|line| line.split_once("error[").map(|(_, code)| code))
        .and_then(|code| code.split_once(']'))
        .map(|(code, _)| code.to_owned());
    Err(code.unwrap_or_else(|| "WRONG".to_owned()))
}

fn run_cell(
    body: &str,
    expected: &str,
    directory: &TestDir,
    index: usize,
) -> [Result<(), String>; 2] {
    let timeout = Duration::from_secs(60);
    let path = directory.join(format!("cell{index}.bn"));
    std::fs::write(&path, source(body)).expect("write cell");
    let mut interpreted = run(bni().arg("run").arg(&path), None, timeout).expect("run bni");
    let interpreted_result = outcome(&interpreted, expected);
    interpreted.accept_expected_status();
    let artifact = directory.join(format!("cell{index}{}", std::env::consts::EXE_SUFFIX));
    let mut built = run(bnc().arg(&path).arg("-o").arg(&artifact), None, timeout).expect("run bnc");
    let compiled_result = if built.status.success() {
        let mut compiled = run(&mut std::process::Command::new(&artifact), None, timeout)
            .expect("run compiled cell");
        let result = outcome(&compiled, expected);
        compiled.accept_expected_status();
        result
    } else {
        let result = outcome(&built, expected);
        built.accept_expected_status();
        result
    };
    [interpreted_result, compiled_result]
}

/// The `arc …` lines `BN_ARC_TRACE=1` writes to standard error.
fn trace(output: &support::ProcessOutput) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.starts_with("arc "))
        .map(str::to_owned)
        .collect()
}

/// `BN_ARC_TRACE=1` (proposal `arc-shared-core-0.6.5`, "Inspection"): the
/// one core writes the same trace for every cell both backends run.
#[test]
fn the_arc_trace_is_the_same_on_both_backends() {
    let directory = TestDir::new("ownership-trace").expect("create test directory");
    let timeout = Duration::from_secs(60);
    let mut differences = Vec::new();
    let mut traced = 0;
    for (index, (cell, body, _)) in CELLS.iter().enumerate() {
        if KNOWN_BNC_FAILURES.iter().any(|(known, _)| known == cell) {
            continue;
        }
        let path = directory.join(format!("cell{index}.bn"));
        std::fs::write(&path, source(body)).expect("write cell");
        let mut interpreted = run(
            bni().env("BN_ARC_TRACE", "1").arg("run").arg(&path),
            None,
            timeout,
        )
        .expect("run bni");
        interpreted.accept_expected_status();
        let artifact = directory.join(format!("cell{index}{}", std::env::consts::EXE_SUFFIX));
        let mut built =
            run(bnc().arg(&path).arg("-o").arg(&artifact), None, timeout).expect("run bnc");
        built.accept_expected_status();
        assert!(built.status.success(), "{cell}: bnc failed");
        let mut compiled = run(
            std::process::Command::new(&artifact).env("BN_ARC_TRACE", "1"),
            None,
            timeout,
        )
        .expect("run compiled cell");
        compiled.accept_expected_status();
        let (interpreted, compiled) = (trace(&interpreted), trace(&compiled));
        if interpreted.is_empty() {
            differences.push(format!("{cell}: no trace"));
        } else if interpreted != compiled {
            differences.push(format!(
                "{cell}:\n  bni {interpreted:?}\n  bnc {compiled:?}"
            ));
        }
        traced += 1;
    }
    assert!(traced > 30, "only {traced} cells traced");
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn objects_are_released_once_where_the_rules_say_on_both_backends() {
    let directory = TestDir::new("ownership-matrix").expect("create test directory");
    // `BN_OWNERSHIP_CELL=<text>` prints the source of every cell whose name
    // contains it, to study one by hand.
    if let Ok(only) = std::env::var("BN_OWNERSHIP_CELL") {
        for (cell, body, expected) in CELLS
            .iter()
            .filter(|(cell, ..)| cell.contains(only.as_str()))
        {
            eprintln!("== {cell}\n{}-- expected\n{expected}", source(body));
        }
    }
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((_, body, expected)) = CELLS.get(index) else {
                            return done;
                        };
                        done.push((index, run_cell(body, expected, &directory, index)));
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
    let mut failing = Vec::new();
    for (index, outcomes) in results {
        let cell = CELLS[index].0;
        for ((backend, known), result) in [("bni", KNOWN_BNI_FAILURES), ("bnc", KNOWN_BNC_FAILURES)]
            .into_iter()
            .zip(outcomes)
        {
            if let Err(reason) = &result {
                failing.push(format!("{backend}    (\"{cell}\", \"{reason}\"),"));
            }
            let listed = known.iter().find(|(listed, _)| *listed == cell);
            match (result, listed) {
                (Ok(()), Some(_)) => report.push(format!(
                    "{backend} `{cell}` now passes: remove it from its known-failure list"
                )),
                (Err(reason), None) => report.push(format!("{backend} `{cell}` fails: {reason}")),
                (Err(reason), Some((_, listed_reason))) if reason != *listed_reason => {
                    report.push(format!(
                        "{backend} `{cell}` fails differently: {reason}, listed {listed_reason}"
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

/// Every cell lowered with explicit ownership passes the validator's balance
/// rule (proposal `arc-shared-core-0.6.5`): each owned reference is consumed
/// exactly once on every path.
#[test]
fn ownership_lowering_balances_every_cell() {
    let directory = TestDir::new("ownership-lowering").expect("create test directory");
    let mut failures = Vec::new();
    for (index, (cell, body, _)) in CELLS.iter().enumerate() {
        let path = directory.join(format!("cell{index}.bn"));
        std::fs::write(&path, source(body)).expect("write cell");
        let graph = bn_frontend::module_graph::load(&path).expect("load cell");
        let models = bn_frontend::semantic::analyze_modules(&graph).expect("analyze cell");
        let lowered = bn_frontend::lowering::lower_graph_validated(&graph, &models);
        match lowered {
            Err(error) => failures.push(format!("{cell}: {}", error.message)),
            Ok(validated) => {
                let module = validated.as_module();
                // The lowering emits ownership operations (every cell moves
                // an object).
                let operations = module
                    .functions
                    .iter()
                    .flat_map(|function| &function.blocks)
                    .flat_map(|block| &block.instructions)
                    .filter(|instruction| {
                        matches!(
                            instruction,
                            bn_ir::Instruction::Take { .. } | bn_ir::Instruction::Retain { .. }
                        ) || matches!(
                            instruction,
                            bn_ir::Instruction::Release {
                                destructor: None,
                                ..
                            }
                        )
                    })
                    .count();
                if operations == 0 {
                    failures.push(format!("{cell}: no ownership operation was lowered"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every program the repository ships (valid fixtures and examples) that
/// semantic analysis accepts lowers to IR that passes the validator,
/// including its ownership balance rule.
#[test]
fn ownership_lowering_balances_every_shipped_program() {
    let mut paths = Vec::new();
    for directory in ["tests/grammar/valid", "examples"] {
        for entry in std::fs::read_dir(directory).expect("read program directory") {
            let path = entry.expect("directory entry").path();
            if path.extension().is_some_and(|extension| extension == "bn") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in &paths {
        let Ok(graph) = bn_frontend::module_graph::load(path) else {
            continue;
        };
        let Ok(models) = bn_frontend::semantic::analyze_modules(&graph) else {
            continue;
        };
        checked += 1;
        let lowered = bn_frontend::lowering::lower_graph_validated(&graph, &models);
        if let Err(error) = lowered {
            failures.push(format!(
                "{}:{}: {}",
                path.display(),
                error.span.start.line,
                error.message
            ));
        }
    }
    assert!(checked > 100, "only {checked} programs were checked");
    assert!(
        failures.is_empty(),
        "{} of {checked}:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `BNDispatch` tasks share the one ARC core: four tasks that create and
/// release 2,000 objects each, at once, destroy every object exactly once on
/// both backends (counted from the `BN_ARC_TRACE` lines; their order across
/// tasks is not fixed).
#[test]
fn concurrent_tasks_destroy_every_object_once_on_both_backends() {
    let directory = TestDir::new("ownership-concurrency").expect("create test directory");
    let timeout = Duration::from_secs(120);
    let path = "tests/grammar/valid/dispatch-objects-concurrently.bn";
    let artifact = directory.join(format!("concurrent{}", std::env::consts::EXE_SUFFIX));
    let mut built = run(bnc().arg(path).arg("-o").arg(&artifact), None, timeout).expect("run bnc");
    built.accept_expected_status();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let mut interpreted = run(
        bni().env("BN_ARC_TRACE", "1").arg("run").arg(path),
        None,
        timeout,
    )
    .expect("run bni");
    interpreted.accept_expected_status();
    let mut compiled = run(
        std::process::Command::new(&artifact).env("BN_ARC_TRACE", "1"),
        None,
        timeout,
    )
    .expect("run the compiled program");
    compiled.accept_expected_status();
    for (backend, output) in [("bni", &interpreted), ("bnc", &compiled)] {
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "done\n",
            "{backend}"
        );
        let lines = trace(output);
        let count = |event: &str| {
            lines
                .iter()
                .filter(|line| line.starts_with(&format!("arc {event} Cell#")))
                .count()
        };
        assert_eq!(count("new"), 8000, "{backend}");
        assert_eq!(count("destroy"), 8000, "{backend}");
    }
}

/// Native memory safety of the ARC operations (Sprint 8, phase A): every
/// cell both backends run, and the concurrent tasks, built with
/// `AddressSanitizer` through the `[toolchain]` clang of `bnc`'s `config.toml`,
/// print the expected output with no sanitizer report (no double release, no
/// use after release). Leak detection stays off: an object a `STATIC` holds
/// lives until the process ends, by the language rules; leaks the toolchain
/// would cause are the ARC verifier's job.
#[cfg(unix)]
#[test]
fn native_ownership_runs_clean_under_address_sanitizer() {
    use std::os::unix::fs::PermissionsExt;
    let directory = TestDir::new("ownership-asan").expect("create test directory");
    let wrapper = directory.join("asan-clang");
    std::fs::write(
        &wrapper,
        "#!/bin/sh\nexec clang -fsanitize=address -fno-omit-frame-pointer \"$@\"\n",
    )
    .expect("write clang wrapper");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
        .expect("make the wrapper executable");
    std::fs::write(
        directory.join("config.toml"),
        format!("[toolchain]\nclang = \"{}\"\n", wrapper.display()),
    )
    .expect("write config.toml");
    let timeout = Duration::from_secs(120);
    let concurrent = std::fs::canonicalize("tests/grammar/valid/dispatch-objects-concurrently.bn")
        .expect("concurrency program");
    let mut programs = CELLS
        .iter()
        .enumerate()
        .filter(|(_, (cell, _, _))| !KNOWN_BNC_FAILURES.iter().any(|(known, _)| known == cell))
        .map(|(index, (cell, body, expected))| {
            let path = directory.join(format!("cell{index}.bn"));
            std::fs::write(&path, source(body)).expect("write cell");
            ((*cell).to_owned(), path, (*expected).to_owned())
        })
        .collect::<Vec<_>>();
    programs.push(("concurrent tasks".into(), concurrent, "done\n".into()));
    let mut failures = Vec::new();
    for (index, (cell, path, expected)) in programs.iter().enumerate() {
        let artifact = directory.join(format!("asan{index}"));
        let mut built = run(
            bnc()
                .current_dir(directory.path())
                .arg(path)
                .arg("-o")
                .arg(&artifact),
            None,
            timeout,
        )
        .expect("run bnc");
        built.accept_expected_status();
        if !built.status.success() {
            failures.push(format!(
                "{cell}: build: {}",
                String::from_utf8_lossy(&built.stderr)
            ));
            continue;
        }
        let mut ran = run(
            std::process::Command::new(&artifact).env("ASAN_OPTIONS", "detect_leaks=0"),
            None,
            timeout,
        )
        .expect("run the sanitized program");
        ran.accept_expected_status();
        let stderr = String::from_utf8_lossy(&ran.stderr);
        if !ran.status.success()
            || stderr.contains("AddressSanitizer")
            || String::from_utf8_lossy(&ran.stdout) != *expected
        {
            failures.push(format!("{cell}: {stderr}"));
        }
    }
    assert!(programs.len() > 30, "only {} programs", programs.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
