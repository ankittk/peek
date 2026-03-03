use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

#[test]
fn help_shows_usage() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("peek").expect("binary peek should build");
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Process intelligence tool for Linux"));
}

#[test]
fn version_prints_cargo_pkg_version() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("peek").expect("binary peek should build");
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn json_for_self_pid_is_valid() {
    let pid = std::process::id() as i32;

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("peek").expect("binary peek should build");
    cmd.args([pid.to_string(), "--json".to_string()]);
    let output = cmd.assert().success().get_output().stdout.clone();

    let v: Value = serde_json::from_slice(&output).expect("stdout should be valid JSON");
    assert_eq!(v["pid"].as_i64().unwrap_or_default(), pid as i64);
}

