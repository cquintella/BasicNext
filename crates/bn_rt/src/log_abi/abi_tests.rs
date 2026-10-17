use super::*;
use std::ffi::CString;

fn bn_string(s: &str) -> *mut c_char {
    crate::string::c_string_from_bytes(s.as_bytes())
}

fn free_bn_string(ptr: *mut c_char) {
    crate::string::bn_rt_str_release(ptr);
}

#[test]
fn a_file_logger_writes_json_records_and_releases_its_handle() {
    let path = std::env::temp_dir().join(format!(
        "bn-log-abi-{}-{}.jsonl",
        std::process::id(),
        registry().next.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let path_text = bn_string(&path.to_string_lossy());
    let label = bn_string("app");
    let json = bn_string("JSON");
    let user = bn_string("ana");
    let message = bn_string("connected");
    let mut logger = 0;
    assert_eq!(
        bn_rt_log_logger_new(label, json, path_text, &raw mut logger),
        BN_LOG_OK
    );
    free_bn_string(path_text);
    free_bn_string(label);
    free_bn_string(json);
    let null = std::ptr::null();
    assert_eq!(
        bn_rt_log_logger_write(logger, 2, message, null, null, null, user, null, 0),
        BN_LOG_OK
    );
    free_bn_string(user);
    free_bn_string(message);
    assert_eq!(bn_rt_log_logger_flush(logger, 1_000), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_close(logger, 1_000), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_close(logger, 1_000), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_delete(logger), BN_LOG_OK);
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("\"message\":\"connected\""));
    assert!(contents.contains("\"user\":\"ana\""));
    std::fs::remove_file(path).unwrap();
}

/// A file logger on `path`, as `Logger.New` makes it in a compiled program.
fn file_logger(path: &std::path::Path) -> u64 {
    let path_text = bn_string(&path.to_string_lossy());
    let label = bn_string("app");
    let json = bn_string("JSON");
    let mut logger = 0;
    assert_eq!(
        bn_rt_log_logger_new(label, json, path_text, &raw mut logger),
        BN_LOG_OK
    );
    free_bn_string(path_text);
    free_bn_string(label);
    free_bn_string(json);
    logger
}

/// One INFO record with no field.
fn info(logger: u64, message: &str) -> i32 {
    let message = bn_string(message);
    let null = std::ptr::null();
    let res = bn_rt_log_logger_write(logger, 2, message, null, null, null, null, null, 0);
    free_bn_string(message);
    res
}

#[cfg(unix)]
#[test]
fn rooted_file_transport_cannot_be_redirected_after_policy_configuration() {
    const CHILD: &str = "BN_LOG_ROOT_SWAP_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "log_abi::tests::rooted_file_transport_cannot_be_redirected_after_policy_configuration",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }

    let base = std::env::temp_dir().join(format!("bn-log-root-swap-{}", std::process::id()));
    let root = base.join("root");
    let moved = base.join("configured-root");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    assert_eq!(super::super::policy::bn_rt_policy_filesystem_sandboxed(), 0);
    let root_name = CString::new(root.to_string_lossy().as_bytes()).unwrap();
    assert_eq!(
        super::super::policy::bn_rt_policy_filesystem_root(1, root_name.as_ptr()),
        0
    );
    let logger = file_logger(&root.join("events.jsonl"));
    std::fs::rename(&root, &moved).unwrap();
    std::os::unix::fs::symlink(&outside, &root).unwrap();
    assert_eq!(info(logger, "safe"), BN_LOG_OK);
    assert!(moved.join("events.jsonl").is_file());
    assert!(!outside.join("events.jsonl").exists());
    std::fs::remove_file(root).unwrap();
    std::fs::remove_dir_all(base).unwrap();
}

#[cfg(unix)]
#[test]
fn file_transport_reports_policy_denial_after_intermediate_symlink_swap() {
    const CHILD: &str = "BN_LOG_PARENT_SWAP_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "log_abi::tests::file_transport_reports_policy_denial_after_intermediate_symlink_swap",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }

    let base = std::env::temp_dir().join(format!("bn-log-parent-swap-{}", std::process::id()));
    let root = base.join("root");
    let live = root.join("live");
    let parked = root.join("parked");
    let outside = base.join("outside");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    assert_eq!(super::super::policy::bn_rt_policy_filesystem_sandboxed(), 0);
    let root_name = CString::new(root.to_string_lossy().as_bytes()).unwrap();
    assert_eq!(
        super::super::policy::bn_rt_policy_filesystem_root(1, root_name.as_ptr()),
        0
    );
    let logger = file_logger(&live.join("events.jsonl"));
    std::fs::rename(&live, &parked).unwrap();
    std::os::unix::fs::symlink(&outside, &live).unwrap();
    assert_eq!(
        info(logger, "denied"),
        bn_types::error_codes::log::IO_FAILED
    );
    assert!(!outside.join("events.jsonl").exists());
    std::fs::remove_file(live).unwrap();
    std::fs::remove_dir_all(base).unwrap();
}

/// A logger lives while a binding holds it: a copy retains it, each binding's
/// end releases one, and the last release destroys it (known issue K61).
#[test]
fn the_last_release_destroys_a_logger() {
    let label = bn_string("app");
    let json = bn_string("JSON");
    let null_dst = bn_string("NULL");
    let mut logger = 0;
    assert_eq!(
        bn_rt_log_logger_new(label, json, null_dst, &raw mut logger),
        BN_LOG_OK
    );
    free_bn_string(label);
    free_bn_string(json);
    free_bn_string(null_dst);
    assert_eq!(bn_rt_log_logger_retain(logger), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_delete(logger), BN_LOG_OK);
    assert_eq!(info(logger, "still held"), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_delete(logger), BN_LOG_OK);
    assert_eq!(
        info(logger, "destroyed"),
        bn_types::error_codes::log::INVALID_ARGUMENT
    );
    // Handle 0 is the error case of `Logger OR Error`: nothing to count.
    assert_eq!(bn_rt_log_logger_retain(0), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_delete(0), BN_LOG_OK);
}
