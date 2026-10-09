use bn_cli::process_log::{LogLevel, LogTarget, ProcessLog};

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
    log.write_to(&LogTarget::Explicit(path.clone()))
        .expect("process log writes");
    let text = std::fs::read_to_string(&path).expect("process log is readable");
    let _ = std::fs::remove_file(path);

    assert!(!text.contains("ignored"));
    assert!(text.contains("accepted"));
    assert!(!text.contains("token=secret"));
}

/// The log never blocks on a FIFO planted at its path; it refuses it.
#[cfg(unix)]
#[test]
fn the_log_refuses_a_fifo() {
    let directory = std::env::temp_dir().join(format!("bn-log-fifo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create directory");
    let fifo = directory.join("fifo.bnbuild.log");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(status.success());
    let mut log = ProcessLog::new(LogLevel::Debug);
    log.event(LogLevel::Info, "pipeline", "start", "x");
    assert!(log.write_to(&LogTarget::Companion(fifo)).is_err());
    std::fs::remove_dir_all(directory).expect("remove directory");
}
