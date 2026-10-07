use std::sync::atomic::Ordering;
use std::time::{Duration, UNIX_EPOCH};

use super::{
    Executor, HostEnv, Instance, Module, Value, coerce, default_span, host_random_seed,
    integer_from_i128_count, is_value, shared_string,
};
use bn_types::{FloatType, IntegerType, Type};

#[test]
fn filesystem_policy_separates_read_and_write_roots() {
    let root = std::env::current_dir().expect("repository root");
    let policy = super::HostEnv::fixed(Vec::new(), 0, 0)
        .with_filesystem_roots(vec![root.clone()], Vec::new())
        .expect("current directory is a valid policy root");
    assert!(
        policy
            .filesystem()
            .allows_path(&root.join("Cargo.toml"), false)
    );
    assert!(
        !policy
            .filesystem()
            .allows_path(&root.join("Cargo.toml"), true)
    );
    assert!(
        !policy
            .filesystem()
            .allows_path(std::path::Path::new("/etc/hosts"), false)
    );
}

#[test]
fn host_env_sandbox_defaults_to_fail_closed_filesystem() {
    let env = super::HostEnv::sandbox(Vec::new());
    assert!(!env.filesystem().allows_capability());
    assert!(
        !env.filesystem()
            .allows_path(std::path::Path::new("/"), false)
    );
    assert!(
        !env.filesystem()
            .allows_path(std::path::Path::new("/etc/hosts"), false)
    );
    assert!(
        !env.filesystem()
            .allows_path(std::path::Path::new("Cargo.toml"), true)
    );
}

#[test]
fn host_env_exec_policy_can_be_denied_independently() {
    assert!(super::HostEnv::system(Vec::new()).exec_allowed());
    assert!(
        !super::HostEnv::system(Vec::new())
            .without_exec()
            .exec_allowed()
    );
    assert!(!super::HostEnv::sandbox(Vec::new()).exec_allowed());
}

#[test]
fn filesystem_policy_mitigates_symlink_escape() {
    let temp_dir = std::env::temp_dir().join(format!("bn_symlink_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let target_file = temp_dir.join("outside.txt");
    std::fs::write(&target_file, "secret").unwrap();

    let sandbox_dir = temp_dir.join("sandbox");
    std::fs::create_dir_all(&sandbox_dir).unwrap();

    #[cfg(unix)]
    {
        let symlink_path = sandbox_dir.join("leak_link.txt");
        let _ = std::os::unix::fs::symlink(&target_file, &symlink_path);
        let policy = super::HostEnv::fixed(Vec::new(), 0, 0)
            .with_filesystem_roots(vec![sandbox_dir.clone()], Vec::new())
            .unwrap();

        // Canonicalization resolves the link to outside.txt, which is outside sandbox_dir -> must be denied!
        assert!(!policy.filesystem().allows_path(&symlink_path, false));
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// The guarantee comes from the Unix `openat`/`O_NOFOLLOW` implementation in
// bn_rt::secure_fs; creating the symlink also needs privileges on Windows.
#[cfg(unix)]
#[test]
fn filesystem_policy_open_cannot_be_redirected_after_root_configuration() {
    use std::io::Read as _;

    let temp_dir =
        std::env::temp_dir().join(format!("bn_symlink_race_test_{}", std::process::id()));
    let root = temp_dir.join("root");
    let moved = temp_dir.join("configured-root");
    let outside = temp_dir.join("outside");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(root.join("value.txt"), "inside").unwrap();
    std::fs::write(outside.join("value.txt"), "outside").unwrap();
    let policy = super::HostEnv::fixed(Vec::new(), 0, 0)
        .with_filesystem_roots(vec![root.clone()], Vec::new())
        .unwrap();
    std::fs::rename(&root, &moved).unwrap();
    std::os::unix::fs::symlink(&outside, &root).unwrap();

    let mut text = String::new();
    policy
        .filesystem()
        .open(&root.join("value.txt"), bn_rt::secure_fs::OpenMode::Read)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "inside");
    assert_eq!(
        std::fs::read_to_string(outside.join("value.txt")).unwrap(),
        "outside"
    );
    std::fs::remove_file(root).unwrap();
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn system_random_seed_is_never_zero() {
    assert_ne!(host_random_seed(), 0);
}

#[test]
fn async_host_forks_receive_independent_random_states() {
    let host = super::HostEnv::fixed(vec!["async.bn".into()], 0, 0);
    let first = host.fork_for_task();
    let second = host.fork_for_task();
    assert_ne!(
        first.random_state.load(Ordering::Relaxed),
        second.random_state.load(Ordering::Relaxed)
    );
}

#[test]
fn async_tasks_do_not_cross_talk_through_host_random_state() {
    let host = super::HostEnv::fixed(vec!["async.bn".into()], 0, 0);
    let first = host.fork_for_task();
    let second = host.fork_for_task();
    let first_values = std::thread::spawn(move || {
        [
            first.random_state.fetch_add(1, Ordering::Relaxed),
            first.random_state.fetch_add(1, Ordering::Relaxed),
        ]
    });
    let second_values = std::thread::spawn(move || {
        [
            second.random_state.fetch_add(1, Ordering::Relaxed),
            second.random_state.fetch_add(1, Ordering::Relaxed),
        ]
    });
    let first_values = first_values.join().expect("first task");
    let second_values = second_values.join().expect("second task");
    assert_eq!(first_values[1], first_values[0] + 1);
    assert_eq!(second_values[1], second_values[0] + 1);
    assert_ne!(first_values[0], second_values[0]);
}

#[test]
fn system_timestamp_before_epoch_is_negative() {
    assert_eq!(
        bn_rt::timestamp_ms_from(UNIX_EPOCH - Duration::from_millis(1)),
        -1
    );
}

#[test]
fn integer_count_rejects_values_above_the_language_limit() {
    let error = integer_from_i128_count(i128::from(i32::MAX) + 1, default_span())
        .expect_err("INTEGER count overflow");
    assert_eq!(error.code, "NUMERIC_OVERFLOW");
}

#[test]
fn network_handles_match_their_explicit_alternative_types() {
    let tcp = coerce(
        Value::TcpListener(1),
        &Type::Alternative(vec![
            Type::Named("HOST.Net.TCPListener".into()),
            Type::Named("Error".into()),
        ]),
        default_span(),
    );
    let udp = coerce(
        Value::UdpSocket(1),
        &Type::Alternative(vec![
            Type::Named("HOST.Net.UDPSocket".into()),
            Type::Named("Error".into()),
        ]),
        default_span(),
    );
    assert!(tcp.is_ok());
    assert!(udp.is_ok());
}

#[test]
fn primitive_type_tests_match_their_runtime_values() {
    assert!(is_value(&Value::Integer(1, IntegerType::Int32), "INTEGER"));
    assert!(is_value(&Value::Integer(1, IntegerType::Byte), "BYTE"));
    assert!(!is_value(&Value::Integer(1, IntegerType::Byte), "INTEGER"));
    assert!(is_value(&Value::Float(1.0, FloatType::Float64), "FLOAT"));
    assert!(is_value(&Value::Boolean(true), "BOOLEAN"));
    assert!(is_value(&Value::String("BN".into()), "STRING"));
}

/// `BN_FS_POLICY=deny` keeps the provider (operations return an Error);
/// only `without_filesystem` removes it (0.6.md).
#[test]
fn a_denying_filesystem_policy_still_provides_the_capability() {
    let host = super::HostEnv::fixed(Vec::new(), 0, 0);
    assert!(host.provides_filesystem());
    assert!(!host.clone().without_filesystem().provides_filesystem());
    let denied = host
        .narrowed_by_env(|name| (name == "BN_FS_POLICY").then(|| "deny".to_owned()))
        .expect("valid policy input");
    assert!(denied.provides_filesystem());
    assert!(!denied.filesystem().allows_capability());
}

/// The ARC verifier (debug builds check every program end): a live object
/// whose count is more than the references the program holds is a leak the
/// toolchain caused, and fails the run; a held object passes.
#[test]
fn the_arc_verifier_reports_a_count_without_references() {
    let mut module = Module::default();
    module.field_layouts.insert(
        "Box".into(),
        bn_ir::FieldLayout {
            owner: "Box".into(),
            fields: Vec::new(),
            span: default_span(),
        },
    );
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let host = HostEnv::system(Vec::new());
    let mut executor = Executor::new(&module, &mut input, &mut output, &host, None, None);
    let held = executor.register_allocation("Box", default_span());
    executor
        .objects
        .insert(
            held,
            1,
            Instance {
                fields: Box::new([]),
            },
            default_span(),
        )
        .expect("payload");
    executor.statics.insert(
        ("Holder".into(), "slot".into()),
        Value::Object {
            handle: held,
            class: shared_string("Box"),
        },
    );
    executor
        .verify_arc(default_span())
        .expect("a STATIC holds the object");
    let leaked = executor.register_allocation("Box", default_span());
    executor
        .objects
        .insert(
            leaked,
            1,
            Instance {
                fields: Box::new([]),
            },
            default_span(),
        )
        .expect("payload");
    let error = executor
        .verify_arc(default_span())
        .expect_err("nothing holds the second object");
    assert_eq!(error.code, "INVALID_IR");
    assert!(
        error.message.contains("Box#1 has strong count 1"),
        "{}",
        error.message
    );
}
