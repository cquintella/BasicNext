use bn_cli::process_log::{LogLevel, ProcessLog};

#[test]
fn parse_rejects_unknown_level() {
    assert!(LogLevel::parse("verbose").is_err());
}

#[test]
fn write_redacts_token_values_and_filters_events() {
    let path = std::env::temp_dir().join(format!(
        "bn-cli-process-log-{}-{}.log",
        std::process::id(),
        std::thread::current().name().unwrap_or("test"),
    ));
    let mut log = ProcessLog::new(LogLevel::Info);
    log.event(LogLevel::Debug, "config", "ignored", "token=secret");
    log.event(LogLevel::Info, "config", "accepted", "token=secret");
    log.write_to(&path).expect("process log writes");
    let text = std::fs::read_to_string(&path).expect("process log is readable");
    let _ = std::fs::remove_file(path);

    assert!(!text.contains("ignored"));
    assert!(text.contains("accepted"));
    assert!(!text.contains("token=secret"));
}
