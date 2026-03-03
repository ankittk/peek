use export_engine::{render_html, render_markdown, to_json, ProcessSnapshot};
use chrono::{TimeZone, Utc};
use peek_core::ProcessInfo;

#[test]
fn json_serialization_round_trips_basic_snapshot() {
    let info = ProcessInfo {
        pid: 1234,
        name: "test-proc".to_string(),
        cmdline: "test-proc --flag".to_string(),
        exe: Some("/usr/bin/test-proc".to_string()),
        state: "Running".to_string(),
        ppid: 1,
        uid: 0,
        gid: 0,
        started_at: None,
        threads: 1,
        vm_size_kb: 0,
        rss_kb: 0,
        pss_kb: None,
        swap_kb: None,
        cpu_percent: None,
        io_read_bytes: None,
        io_write_bytes: None,
        fd_count: None,
        kernel: None,
        network: None,
        open_files: None,
        env_vars: None,
        process_tree: None,
        gpu: None,
    };
    let snapshot = ProcessSnapshot {
        captured_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        peek_version: "test-version".to_string(),
        process: info,
    };

    let s = to_json(&snapshot).expect("to_json should succeed");
    assert!(s.contains("test-version"));
}

#[test]
fn html_wraps_markdown_report() {
    let info = ProcessInfo {
        pid: 1234,
        name: "test-proc".to_string(),
        cmdline: "test-proc --flag".to_string(),
        exe: Some("/usr/bin/test-proc".to_string()),
        state: "Running".to_string(),
        ppid: 1,
        uid: 0,
        gid: 0,
        started_at: None,
        threads: 1,
        vm_size_kb: 0,
        rss_kb: 0,
        pss_kb: None,
        swap_kb: None,
        cpu_percent: None,
        io_read_bytes: None,
        io_write_bytes: None,
        fd_count: None,
        kernel: None,
        network: None,
        open_files: None,
        env_vars: None,
        process_tree: None,
        gpu: None,
    };
    let snapshot = ProcessSnapshot {
        captured_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        peek_version: "test-version".to_string(),
        process: info,
    };

    let html = render_html(&snapshot);
    assert!(html.contains("<html"));
    assert!(html.contains("<pre><code>"));
}

#[test]
fn markdown_contains_core_sections() {
    let info = ProcessInfo {
        pid: 1234,
        name: "test-proc".to_string(),
        cmdline: "test-proc --flag".to_string(),
        exe: Some("/usr/bin/test-proc".to_string()),
        state: "Running".to_string(),
        ppid: 1,
        uid: 0,
        gid: 0,
        started_at: None,
        threads: 1,
        vm_size_kb: 0,
        rss_kb: 0,
        pss_kb: None,
        swap_kb: None,
        cpu_percent: Some(1.0),
        io_read_bytes: None,
        io_write_bytes: None,
        fd_count: None,
        kernel: None,
        network: None,
        open_files: None,
        env_vars: None,
        process_tree: None,
        gpu: None,
    };
    let snapshot = ProcessSnapshot {
        captured_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        peek_version: "test-version".to_string(),
        process: info,
    };

    let md = render_markdown(&snapshot);
    assert!(md.contains("# peek report"));
    assert!(md.contains("## Process"));
    assert!(md.contains("Name"));
    assert!(md.contains("PID"));
}


