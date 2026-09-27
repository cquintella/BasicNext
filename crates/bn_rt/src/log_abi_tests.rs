use super::*;
use std::ffi::CString;

#[test]
fn file_transport_writes_redacted_json_and_releases_handles() {
    let path = std::env::temp_dir().join(format!(
        "bn-log-abi-{}-{}.jsonl",
        std::process::id(),
        next_handle()
    ));
    let _ = std::fs::remove_file(&path);
    let path_text = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let key = CString::new("authorization").unwrap();
    let secret = CString::new("do-not-write").unwrap();
    let message = CString::new("connected").unwrap();
    let fields = bn_rt_log_fields_create();
    let logger = bn_rt_log_logger_create();
    assert_eq!(
        bn_rt_log_fields_set_string(fields, key.as_ptr(), secret.as_ptr()),
        BN_LOG_OK
    );
    assert_eq!(
        bn_rt_log_logger_add_file(logger, path_text.as_ptr(), 2),
        BN_LOG_OK
    );
    assert_eq!(
        bn_rt_log_logger_log(logger, 2, message.as_ptr(), fields),
        BN_LOG_OK
    );
    assert_eq!(bn_rt_log_logger_flush(logger, 1_000), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_close(logger, 1_000), BN_LOG_OK);
    assert_eq!(bn_rt_log_fields_close(fields), BN_LOG_OK);
    assert_eq!(bn_rt_log_logger_delete(logger), BN_LOG_OK);
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("\"message\":\"connected\""));
    assert!(!contents.contains("do-not-write"));
    std::fs::remove_file(path).unwrap();
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
    let log_path = root.join("events.jsonl");
    let path_text = CString::new(log_path.to_string_lossy().as_bytes()).unwrap();
    let message = CString::new("safe").unwrap();
    let fields = bn_rt_log_fields_create();
    let logger = bn_rt_log_logger_create();
    assert_eq!(
        bn_rt_log_logger_add_file(logger, path_text.as_ptr(), 2),
        BN_LOG_OK
    );
    std::fs::rename(&root, &moved).unwrap();
    std::os::unix::fs::symlink(&outside, &root).unwrap();
    assert_eq!(
        bn_rt_log_logger_log(logger, 2, message.as_ptr(), fields),
        BN_LOG_OK
    );
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
    let log_path = live.join("events.jsonl");
    let path_text = CString::new(log_path.to_string_lossy().as_bytes()).unwrap();
    let logger = bn_rt_log_logger_create();
    assert_eq!(
        bn_rt_log_logger_add_file(logger, path_text.as_ptr(), 2),
        BN_LOG_OK
    );
    std::fs::rename(&live, &parked).unwrap();
    std::os::unix::fs::symlink(&outside, &live).unwrap();

    let message = CString::new("denied").unwrap();
    let fields = bn_rt_log_fields_create();
    assert_eq!(
        bn_rt_log_logger_log(logger, 2, message.as_ptr(), fields),
        BN_LOG_POLICY_DENIED
    );
    assert!(!outside.join("events.jsonl").exists());
    std::fs::remove_file(live).unwrap();
    std::fs::remove_dir_all(base).unwrap();
}
