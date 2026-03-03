use std::env;
use std::path::PathBuf;

use clap::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    // Minimal command definition just for man page generation.
    let cmd = Command::new("peekd")
        .about("Background daemon for peek resource history and alerting")
        .long_about(
            "peekd is the optional background daemon that powers peek's --history and alert \
rules.\n\n\
It samples watched processes into an in-memory ring buffer (with optional JSONL history on \
disk), evaluates alert rules from alerts.toml and the CLI, and exposes a Unix socket \
for the peek CLI to query.",
        )
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .disable_version_flag(true);

    let man = clap_mangen::Man::new(cmd);
    let mut buf: Vec<u8> = Vec::new();
    man.render(&mut buf)?;

    let man_path = out_dir.join("peekd.8");
    std::fs::write(&man_path, &buf)?;
    println!("cargo:warning=peekd man page written to {}", man_path.display());

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}

