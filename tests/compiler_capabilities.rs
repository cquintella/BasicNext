mod support;

use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bn_support_matrix::build_report;
use serde::Deserialize;
use support::{ProcessOutput, TestDir, bnc, bni, run, workspace_root};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    programs: Vec<Program>,
}

#[derive(Debug, Deserialize)]
struct Program {
    path: String,
    support: String,
    exit_code: i32,
    #[serde(default)]
    stdout: Option<String>,
    id: String,
    target: String,
    op: String,
    ir_instructions: Vec<String>,
    type_constraints: Vec<String>,
    conditions: Vec<String>,
    tests: Vec<String>,
    reject_diag: Option<String>,
    provider: String,
    evidence: String,
    #[serde(default)]
    build_diagnostic_contains: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    stdin: String,
    #[serde(default)]
    run_exit_codes: Option<Vec<i32>>,
    #[serde(default)]
    run_stdout_contains: Option<String>,
    #[serde(default)]
    observation: Option<String>,
}

fn manifest() -> Manifest {
    serde_json::from_slice(
        &fs::read(workspace_root().join("tests/compiler-capabilities.json"))
            .expect("read capability catalog"),
    )
    .expect("parse typed capability catalog")
}

fn execute(command: &mut Command, input: Option<&[u8]>) -> ProcessOutput {
    run(command, input, PROCESS_TIMEOUT).expect("run child process")
}

fn assert_success(output: &ProcessOutput, context: &str) {
    assert!(
        output.status.success(),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn manifest_has_valid_paths_and_support_labels() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, 1);
    assert!(!manifest.programs.is_empty());
    let unique_paths: HashSet<_> = manifest
        .programs
        .iter()
        .map(|program| &program.path)
        .collect();
    let unique_ids: HashSet<_> = manifest
        .programs
        .iter()
        .map(|program| &program.id)
        .collect();
    assert_eq!(unique_paths.len(), manifest.programs.len());
    assert_eq!(unique_ids.len(), manifest.programs.len());

    for program in &manifest.programs {
        assert!(
            workspace_root().join(&program.path).is_file(),
            "{}",
            program.path
        );
        assert!(matches!(
            program.support.as_str(),
            "llvm-supported" | "llvm-deferred"
        ));
        assert_eq!(program.target, "llvm-native", "{}", program.path);
        assert!(!program.op.contains("program.fixture"), "{}", program.path);
        assert!(
            !program
                .type_constraints
                .iter()
                .any(|item| item.contains("fixture-defined"))
        );
        assert!(
            !program
                .conditions
                .iter()
                .any(|item| item.contains("fixture-defined"))
        );
        assert!(matches!(program.provider.as_str(), "language" | "bn_rt"));
        assert_eq!(program.evidence, "fixture-exact", "{}", program.path);
        assert!(!program.tests.is_empty(), "{}", program.path);
        assert!(!program.ir_instructions.is_empty(), "{}", program.path);
        let mut sorted = program.ir_instructions.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(program.ir_instructions, sorted, "{}", program.path);

        if program.support == "llvm-deferred" {
            let diagnostic = program.reject_diag.as_deref().unwrap_or_default();
            assert!(
                diagnostic.starts_with("TARGET_UNSUPPORTED_")
                    && diagnostic
                        .strip_prefix("TARGET_UNSUPPORTED_")
                        .is_some_and(|suffix| {
                            !suffix.is_empty()
                                && suffix.chars().all(|character| {
                                    character.is_ascii_uppercase() || character == '_'
                                })
                        }),
                "{}",
                program.path
            );
            for prefix in ["Owner:", "Defer-until:", "Risk:"] {
                assert!(
                    program.conditions.iter().any(|condition| {
                        condition
                            .strip_prefix(prefix)
                            .is_some_and(|value| !value.trim().is_empty())
                    }),
                    "{} lacks {prefix}",
                    program.path
                );
            }
            assert_eq!(
                program.build_diagnostic_contains.as_deref(),
                Some(format!("error[{diagnostic}]").as_str()),
                "{}",
                program.path
            );
        }
    }
}

#[test]
fn declared_capabilities_match_user_visible_commands() {
    for program in manifest().programs {
        verify_capability(&program);
    }
}

fn verify_capability(program: &Program) {
    let path = workspace_root().join(&program.path);
    let checked = execute(bni().arg("check").arg(&path), None);
    assert_success(&checked, &format!("check {}", program.path));

    let started = SystemTime::now();
    let mut interpret_command = bni();
    interpret_command
        .arg("run")
        .arg(&path)
        .arg("--")
        .args(&program.args);
    let mut interpreted = execute(&mut interpret_command, Some(program.stdin.as_bytes()));
    let finished = SystemTime::now();
    let expected_codes = program
        .run_exit_codes
        .clone()
        .unwrap_or_else(|| vec![program.exit_code]);
    assert_expected_execution(program, &interpreted, &expected_codes);
    interpreted.accept_expected_status();

    let directory = TestDir::new("capability").expect("create capability directory");
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let mut compile_command = bnc();
    compile_command.arg(&path).arg("-o").arg(&artifact);
    let mut built = execute(&mut compile_command, None);
    if program.support == "llvm-supported" {
        assert_success(&built, &format!("build {}", program.path));
        verify_supported_capability(
            program,
            &artifact,
            &expected_codes,
            &interpreted,
            started,
            finished,
        );
    } else {
        verify_deferred_capability(program, &mut built);
    }
}

fn assert_expected_execution(program: &Program, output: &ProcessOutput, expected_codes: &[i32]) {
    assert!(
        output
            .status
            .code()
            .is_some_and(|code| expected_codes.contains(&code)),
        "{}: {}",
        program.path,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!contains(&output.stdout, b"FAIL"), "{}", program.path);
    if let Some(fragment) = &program.run_stdout_contains {
        assert!(
            contains(&output.stdout, fragment.as_bytes()),
            "{}",
            program.path
        );
    }
}

fn verify_supported_capability(
    program: &Program,
    artifact: &Path,
    expected_codes: &[i32],
    interpreted: &ProcessOutput,
    started: SystemTime,
    finished: SystemTime,
) {
    let native_started = SystemTime::now();
    let mut compiled = execute(
        Command::new(artifact).args(&program.args),
        Some(program.stdin.as_bytes()),
    );
    let native_finished = SystemTime::now();
    assert_expected_execution(program, &compiled, expected_codes);
    compiled.accept_expected_status();
    assert_observation(
        program,
        interpreted,
        &compiled,
        (started, finished),
        (native_started, native_finished),
    );
    if let Some(expected) = &program.stdout {
        assert_eq!(interpreted.stdout, expected.as_bytes(), "{}", program.path);
    }
}

fn assert_observation(
    program: &Program,
    interpreted: &ProcessOutput,
    compiled: &ProcessOutput,
    interpreted_times: (SystemTime, SystemTime),
    compiled_times: (SystemTime, SystemTime),
) {
    match program.observation.as_deref() {
        Some("utc-clock") => {
            assert_utc_clock(
                &interpreted.stdout,
                interpreted_times.0,
                interpreted_times.1,
                &program.path,
            );
            assert_utc_clock(
                &compiled.stdout,
                compiled_times.0,
                compiled_times.1,
                &program.path,
            );
        }
        Some("language-tour") => assert_eq!(
            normalize_language_tour(&compiled.stdout, &program.path),
            normalize_language_tour(&interpreted.stdout, &program.path),
            "{}",
            program.path
        ),
        Some("unordered-prefix-lines") => {
            let mut interpreted_lines = output_lines(&interpreted.stdout);
            let mut compiled_lines = output_lines(&compiled.stdout);
            assert_eq!(
                interpreted_lines.len(),
                compiled_lines.len(),
                "{}",
                program.path
            );
            assert_eq!(
                interpreted_lines.pop(),
                compiled_lines.pop(),
                "{}",
                program.path
            );
            interpreted_lines.sort();
            compiled_lines.sort();
            assert_eq!(interpreted_lines, compiled_lines, "{}", program.path);
        }
        Some("environment-dependent-network") => {
            assert!(!interpreted.stdout.is_empty(), "{}", program.path);
            assert!(!compiled.stdout.is_empty(), "{}", program.path);
        }
        Some(observation) => panic!("unknown observation {observation} for {}", program.path),
        None => assert_eq!(compiled.stdout, interpreted.stdout, "{}", program.path),
    }
}

fn verify_deferred_capability(program: &Program, built: &mut ProcessOutput) {
    assert!(!built.status.success(), "{}", program.path);
    built.accept_expected_status();
    let diagnostics = String::from_utf8_lossy(&built.stderr);
    let expected = program
        .build_diagnostic_contains
        .as_deref()
        .expect("deferred program build diagnostic");
    assert!(diagnostics.contains(expected), "{}", program.path);
    assert_eq!(
        diagnostic_codes(&diagnostics),
        BTreeSet::from([program
            .reject_diag
            .clone()
            .expect("deferred diagnostic code")]),
        "{}",
        program.path
    );
}

#[test]
fn catalogued_ir_inventory_matches_lowered_fixture() {
    for program in manifest().programs {
        let path = workspace_root().join(&program.path);
        let emitted = execute(bni().args(["check", "--emit", "ir"]).arg(path), None);
        assert_success(&emitted, &format!("emit IR for {}", program.path));
        assert_eq!(
            ir_instruction_names(&String::from_utf8_lossy(&emitted.stdout)),
            program.ir_instructions,
            "{}",
            program.path
        );
    }
}

#[test]
fn support_matrix_gap_report_covers_the_ir_and_target_space() {
    let report = build_report(&workspace_root()).expect("build support-matrix report");
    assert_eq!(report.schema_version, 1);
    assert_eq!(
        report.inventory_count,
        report.instruction_count * report.type_count * report.targets.len()
    );
    assert!(report.gap_count > 0);
    assert!(report.covered_count > 0);
    let print_pointer = report
        .inventory
        .iter()
        .find(|entry| {
            entry.instruction == "Print"
                && entry.r#type == "Pointer"
                && entry.target == "llvm-native"
        })
        .expect("Print/Pointer/llvm-native inventory row");
    assert!(print_pointer.evidence.is_empty());
}

#[test]
fn llvm_declared_symbols_have_runtime_exports_and_abi_groups() {
    let declared = llvm_declared_runtime_symbols();
    let exported = runtime_exported_symbols();
    let documented = abi_documented_symbols();
    assert!(!declared.is_empty());
    assert_eq!(
        declared.difference(&exported).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
    assert_eq!(
        declared.difference(&documented).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn diagnostic_codes(diagnostics: &str) -> BTreeSet<String> {
    diagnostics
        .split("error[")
        .skip(1)
        .filter_map(|tail| tail.split_once(']').map(|(code, _)| code.to_owned()))
        .collect()
}

fn ir_instruction_names(ir: &str) -> Vec<String> {
    let mut names: Vec<_> = ir
        .lines()
        .filter_map(|line| line.strip_prefix("                        "))
        .filter_map(|line| line.split_once(" {").map(|(name, _)| name))
        .filter(|name| {
            name.starts_with(char::is_uppercase)
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
}

fn output_lines(output: &[u8]) -> Vec<Vec<u8>> {
    let mut lines: Vec<_> = output
        .split(|byte| *byte == b'\n')
        .map(<[u8]>::to_vec)
        .collect();
    if lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

fn normalize_language_tour(output: &[u8], path: &str) -> Vec<String> {
    let text = String::from_utf8(output.to_vec()).expect("language tour emits UTF-8");
    let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 15, "{path}");

    let mut clock_fields: Vec<_> = lines[5].split_whitespace().collect();
    assert_eq!(clock_fields.len(), 7, "{path}");
    assert!(
        clock_fields[5].parse::<u128>().is_ok_and(|value| value > 0),
        "{path}"
    );
    assert!(clock_fields[6].parse::<u128>().is_ok(), "{path}");
    clock_fields[5] = "<timestamp>";
    clock_fields[6] = "<monotonic>";
    lines[5] = clock_fields.join(" ");

    let mut argument_fields: Vec<_> = lines[6].splitn(3, char::is_whitespace).collect();
    assert_eq!(argument_fields.first(), Some(&"1"), "{path}");
    assert!(
        argument_fields
            .get(1)
            .is_some_and(|value| !value.is_empty()),
        "{path}"
    );
    argument_fields[1] = "<program>";
    lines[6] = argument_fields.join(" ");

    let temporal_fields: Vec<_> = lines[7].split_whitespace().collect();
    assert_eq!(temporal_fields.len(), 2, "{path}");
    assert!(
        temporal_fields[0].parse::<u8>().is_ok_and(|hour| hour < 24),
        "{path}"
    );
    assert!(
        temporal_fields[1]
            .parse::<u8>()
            .is_ok_and(|weekday| (1..=7).contains(&weekday)),
        "{path}"
    );
    "<derived-temporal>".clone_into(&mut lines[7]);
    lines
}

fn assert_utc_clock(output: &[u8], before: SystemTime, after: SystemTime, path: &str) {
    let text = String::from_utf8(output.to_vec()).expect("clock output is UTF-8");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{path}");
    let date = lines[0].strip_prefix("Data:  ").expect("clock date prefix");
    let time = lines[1].strip_prefix("Hora:  ").expect("clock time prefix");
    let date_fields: Vec<i64> = date
        .split('-')
        .map(|field| field.parse().expect("numeric clock date"))
        .collect();
    let time_and_fraction: Vec<_> = time.split('.').collect();
    let time_fields: Vec<i64> = time_and_fraction[0]
        .split(':')
        .map(|field| field.parse().expect("numeric clock time"))
        .collect();
    assert_eq!(date_fields.len(), 3, "{path}");
    assert_eq!(time_fields.len(), 3, "{path}");
    let fraction = time_and_fraction
        .get(1)
        .expect("clock fractional seconds")
        .parse::<i128>()
        .expect("numeric fractional seconds");
    let fraction_digits =
        u32::try_from(time_and_fraction[1].len()).expect("fraction length fits u32");
    let milliseconds = i128::from(days_from_civil(
        date_fields[0],
        date_fields[1],
        date_fields[2],
    )) * 86_400_000
        + i128::from(time_fields[0]) * 3_600_000
        + i128::from(time_fields[1]) * 60_000
        + i128::from(time_fields[2]) * 1_000
        + fraction * 1_000 / 10_i128.pow(fraction_digits);
    let before_ms = system_time_millis(before) - 1;
    let after_ms = system_time_millis(after);
    assert!(
        milliseconds >= before_ms,
        "{path}: {milliseconds} < {before_ms}"
    );
    assert!(
        milliseconds <= after_ms,
        "{path}: {milliseconds} > {after_ms}"
    );
}

fn system_time_millis(time: SystemTime) -> i128 {
    time.duration_since(UNIX_EPOCH)
        .expect("current time follows Unix epoch")
        .as_millis()
        .try_into()
        .expect("current timestamp fits i128")
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn llvm_declared_runtime_symbols() -> BTreeSet<String> {
    [
        "crates/bn_llvm/src/llvm/runtime.rs",
        "crates/bn_llvm/src/llvm/math.rs",
    ]
    .into_iter()
    .flat_map(|path| {
        let source = fs::read_to_string(workspace_root().join(path)).expect("read LLVM source");
        symbols_after_marker(&source, "@bn_rt_", '(')
    })
    .collect()
}

fn runtime_exported_symbols() -> BTreeSet<String> {
    let archive = std::env::var_os("BN_RT_LIB").map_or_else(
        || workspace_root().join("target/debug/libbn_rt.a"),
        PathBuf::from,
    );
    let nm = find_executable("llvm-nm")
        .or_else(|| find_executable("nm"))
        .expect("llvm-nm or nm is required for the ABI export gate");
    let output = execute(Command::new(nm).arg("-g").arg(archive), None);
    assert_success(&output, "inspect bn_rt archive");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.windows(2).find_map(|pair| {
                (pair[0] == "T")
                    .then(|| pair[1].trim_start_matches('_'))
                    .filter(|name| name.starts_with("bn_rt_"))
                    .map(str::to_owned)
            })
        })
        .collect()
}

fn abi_documented_symbols() -> BTreeSet<String> {
    let contract =
        fs::read_to_string(workspace_root().join("docs/architecture/value-memory-abi.md"))
            .expect("read ABI contract");
    let marker = "```text llvm-emitted-bn-rt-symbols\n";
    contract
        .split_once(marker)
        .and_then(|(_, tail)| tail.split_once("\n```").map(|(body, _)| body))
        .map(|body| symbols_after_marker(body, "bn_rt_", '\0'))
        .unwrap_or_default()
}

fn symbols_after_marker(source: &str, marker: &str, required_suffix: char) -> BTreeSet<String> {
    let mut symbols = BTreeSet::new();
    for tail in source.split(marker).skip(1) {
        let suffix: String = tail
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if suffix.is_empty() {
            continue;
        }
        if required_suffix != '\0' && !tail[suffix.len()..].starts_with(required_suffix) {
            continue;
        }
        symbols.insert(format!("bn_rt_{suffix}"));
    }
    symbols
}

fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(name))
            .find(|path| path.is_file())
    })
}
