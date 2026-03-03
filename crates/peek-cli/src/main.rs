mod args;
mod colors;
#[cfg(unix)]
mod peekd_client;
mod tui;

use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use args::Cli;
use chrono::Utc;
use clap::Parser;
use export_engine::{export_pdf, render_html, render_markdown, to_json, ProcessSnapshot};
#[cfg(target_os = "linux")]
use nix::sys::signal::{kill, Signal};
#[cfg(target_os = "linux")]
use nix::unistd::Pid;
#[cfg(target_os = "linux")]
use signal_engine::signals::SIGNAL_MENU;
use peek_core::{
    collect, collect_extended, signal_impact, CollectOptions, PeekError, ProcessInfo, ProcessNode,
};

fn main() {
    let mut cli = Cli::parse();
    apply_config_defaults(&mut cli);
    init_color_from_env_and_cli(&cli);

    if let Err(err) = run(&cli) {
        eprintln!("{} {:#}", colors::red_bold("peek: error:"), err);

        if cli.verbose {
            for (idx, cause) in err.chain().enumerate().skip(1) {
                if idx == 1 {
                    eprintln!("  Caused by: {}", cause);
                } else {
                    eprintln!("             {}", cause);
                }
            }
            eprintln!(
                "{}",
                colors::dimmed("Hint: set RUST_BACKTRACE=1 for a backtrace if this was unexpected.")
            );
        }

        let code = exit_code_for_error(&err);
        std::process::exit(code);
    }
}

/// Apply config file defaults to CLI (no_color, resolve, default_format). Called from main before init_color.
fn apply_config_defaults(cli: &mut Cli) {
    if let Some(cfg) = peek_core::config::load_config() {
        if cfg.defaults.no_color {
            cli.no_color = true;
        }
        if cfg.defaults.resolve {
            cli.resolve = true;
        }
        if cli.export.is_none() && !cli.json && !cli.json_snapshot {
            if let Some(ref fmt) = cfg.export.default_format {
                match fmt.to_lowercase().as_str() {
                    "json" => cli.json = true,
                    "json-snapshot" | "json_snapshot" => cli.json_snapshot = true,
                    "md" | "markdown" => cli.export = Some("md".to_string()),
                    "html" => cli.export = Some("html".to_string()),
                    "pdf" => cli.export = Some("pdf".to_string()),
                    _ => {}
                }
            }
        }
    }
}

fn run(cli_args: &Cli) -> Result<()> {
    // Work on a local, mutable copy so we can honour env defaults.
    let mut cli = cli_args.clone();

    // Configure network sampling window for this invocation if not already set.
    // Default for non-watch (non-TUI) flows is to skip rate sampling (0ms) to
    // avoid blocking quick reports; TUI callers can override via --net-sample-ms
    // or PEEK_NET_SAMPLE_MS.
    if std::env::var("PEEK_NET_SAMPLE_MS").is_err() && cli.watch.is_none() {
        std::env::set_var("PEEK_NET_SAMPLE_MS", "0");
    }
    if let Some(ms) = cli.net_sample_ms {
        std::env::set_var("PEEK_NET_SAMPLE_MS", ms.to_string());
    }

    // PEEK_DEFAULT_FORMAT: default output when user didn't pass explicit format flags.
    if cli.export.is_none() && !cli.json && !cli.json_snapshot {
        if let Ok(fmt) = std::env::var("PEEK_DEFAULT_FORMAT") {
            match fmt.to_lowercase().as_str() {
                "json" => cli.json = true,
                "json-snapshot" | "json_snapshot" => cli.json_snapshot = true,
                "md" | "markdown" => cli.export = Some("md".to_string()),
                "html" => cli.export = Some("html".to_string()),
                "pdf" => cli.export = Some("pdf".to_string()),
                _ => {}
            }
        }
    }

    // Port search mode (does not require a target PID/name)
    if let Some(port) = cli.port {
        return run_port_search(port);
    }

    // Alert management (requires peekd; no target for list/remove)
    if cli.alert_list {
        return run_alert_list();
    }
    if let Some(ref rule_id) = cli.alert_remove {
        return run_alert_remove(rule_id);
    }

    if cli.all {
        cli.resources = true;
        cli.kernel = true;
        cli.network = true;
        cli.files = true;
        cli.env = true;
        cli.tree = true;
    }

    // Re-launch under sudo
    if cli.sudo {
        let exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().collect();
        let filtered: Vec<String> = args[1..]
            .iter()
            .filter(|a| *a != "--sudo")
            .cloned()
            .collect();

        // Re-run the current binary via sudo using its full path, so it works
        // even when installed into a user-local directory (e.g. ~/.local/bin).
        let mut sudo_args = Vec::with_capacity(1 + filtered.len());
        sudo_args.push(exe.to_string_lossy().into_owned());
        sudo_args.extend(filtered);

        let status = std::process::Command::new("sudo")
            .args(&sudo_args)
            .status()?;
        std::process::exit(status.code().unwrap_or(1));
    }

    let had_no_target = cli.target.is_none();
    let pid = resolve_target(&cli)?;

    // Interactive mode (peek with no args): after picking a process, default to live TUI
    if had_no_target && cli.watch.is_none() && cli.export.is_none() && !cli.kill
        && !cli.json && !cli.json_snapshot && !cli.history && cli.diff.is_none()
        && cli.alert_add.is_none()
    {
        return tui::run_tui(pid, &cli, Duration::from_millis(2000));
    }

    // --alert-add: add alert rule for this PID
    if let Some(ref args) = cli.alert_add {
        return run_alert_add(pid, args);
    }

    // --diff: side-by-side comparison
    if let Some(pid2) = cli.diff {
        return run_diff(pid, pid2);
    }

    // --history: fetch from peekd (Unix/Linux only)
    if cli.history {
        return run_history(pid, &cli);
    }

    // --watch: live TUI
    if let Some(ref interval_opt) = cli.watch {
        let ms = interval_opt.unwrap_or(2000);
        return tui::run_tui(pid, &cli, Duration::from_millis(ms));
    }

    // For md/html export include process tree in the snapshot
    let opts = if cli.export.as_deref().map(|s| s.to_lowercase()).as_deref() == Some("md")
        || cli.export.as_deref().map(|s| s.to_lowercase()).as_deref() == Some("markdown")
        || cli.export.as_deref().map(|s| s.to_lowercase()).as_deref() == Some("html")
    {
        let mut o = make_opts(&cli);
        o.tree = true;
        o
    } else {
        make_opts(&cli)
    };
    let info = collect_extended(pid, &opts).map_err(map_core_error)?;

    // --export
    if let Some(ref fmt) = cli.export {
        let snapshot = ProcessSnapshot {
            captured_at: Utc::now(),
            peek_version: env!("CARGO_PKG_VERSION").to_string(),
            process: info.clone(),
        };
        return run_export(&snapshot, fmt);
    }

    // --kill
    if cli.kill {
        return run_kill_panel(pid, &info);
    }

    // --json-snapshot
    if cli.json_snapshot {
        let snapshot = ProcessSnapshot {
            captured_at: Utc::now(),
            peek_version: env!("CARGO_PKG_VERSION").to_string(),
            process: info.clone(),
        };
        println!("{}", to_json(&snapshot)?);
        return Ok(());
    }

    // --json (backwards-compatible raw ProcessInfo)
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }

    print_report(&info, &cli);
    Ok(())
}

/// Decide when to disable colours.
///
/// Rules:
/// - `--no-color` or `PEEK_NO_COLOR` → force disable (set NO_COLOR).
/// - Existing `NO_COLOR` is respected.
/// - If stdout is not a TTY and no override is set, disable colours by default.
fn init_color_from_env_and_cli(cli: &Cli) {
    use std::io::IsTerminal;

    if cli.no_color || std::env::var_os("PEEK_NO_COLOR").is_some() {
        std::env::set_var("NO_COLOR", "1");
        return;
    }

    if std::env::var_os("NO_COLOR").is_some() {
        return;
    }

    if !io::stdout().is_terminal() {
        std::env::set_var("NO_COLOR", "1");
    }
}

// ─── Opts helper ─────────────────────────────────────────────────────────────

fn make_opts(cli: &Cli) -> CollectOptions {
    CollectOptions {
        resources: cli.resources || cli.all,
        kernel: cli.kernel || cli.all,
        network: cli.network || cli.all,
        files: cli.files || cli.all,
        env: cli.env || cli.all,
        tree: cli.tree || cli.all,
        gpu: cli.all,
    }
}

// ─── Diff ─────────────────────────────────────────────────────────────────────

fn run_diff(pid1: i32, pid2: i32) -> Result<()> {
    let i1 = collect(pid1).map_err(map_core_error)?;
    let i2 = collect(pid2).map_err(map_core_error)?;

    println!("{}", colors::bold("PROCESS COMPARISON"));
    println!(
        "{:<28} {:>22} {:>22}",
        colors::bold("Field"),
        format!("{} ({})", colors::cyan_bold(&i1.name), pid1),
        format!("{} ({})", colors::cyan_bold(&i2.name), pid2)
    );
    println!("{}", "─".repeat(74));

    macro_rules! row {
        ($label:expr, $v1:expr, $v2:expr) => {
            println!("{:<28} {:>22} {:>22}", $label, $v1, $v2);
        };
    }

    row!("State", &i1.state, &i2.state);
    row!("PPID", i1.ppid.to_string(), i2.ppid.to_string());
    row!(
        "UID:GID",
        format!("{}:{}", i1.uid, i1.gid),
        format!("{}:{}", i2.uid, i2.gid)
    );
    row!("Threads", i1.threads.to_string(), i2.threads.to_string());
    row!("RSS (KB)", i1.rss_kb.to_string(), i2.rss_kb.to_string());
    row!(
        "VSZ (KB)",
        i1.vm_size_kb.to_string(),
        i2.vm_size_kb.to_string()
    );

    // Delta row for memory
    let rss_delta = i2.rss_kb as i64 - i1.rss_kb as i64;
    let delta_str = if rss_delta >= 0 {
        colors::yellow(&format!("+{} KB", rss_delta))
    } else {
        colors::green(&format!("{} KB", rss_delta))
    };
    println!("{:<28} {:>44}", "RSS delta (pid2 - pid1)", delta_str);

    Ok(())
}

// ─── Alerts (peekd) ───────────────────────────────────────────────────────────

#[cfg(not(unix))]
fn run_alert_list() -> Result<()> {
    eprintln!(
        "{}",
        colors::yellow("peekd (alerts) is only available on Linux/Unix.")
    );
    std::process::exit(1);
}

#[cfg(not(unix))]
fn run_alert_remove(_rule_id: &str) -> Result<()> {
    eprintln!(
        "{}",
        colors::yellow("peekd (alerts) is only available on Linux/Unix.")
    );
    std::process::exit(1);
}

#[cfg(not(unix))]
fn run_alert_add(_pid: i32, _args: &[String]) -> Result<()> {
    eprintln!(
        "{}",
        colors::yellow("peekd (alerts) is only available on Linux/Unix.")
    );
    std::process::exit(1);
}

#[cfg(unix)]
fn run_alert_list() -> Result<()> {
    if !peekd_client::ping() {
        eprintln!(
            "{}",
            colors::yellow("peekd is not running. Start it with: sudo systemctl start peekd (or from repo: sudo mkdir -p /run/peekd && sudo ./target/release/peekd &)")
        );
        std::process::exit(1);
    }
    let rules = peekd_client::alert_list()?;
    if rules.is_empty() {
        println!("No alert rules.");
        return Ok(());
    }
    println!();
    println!("{}", colors::bold("ALERT RULES"));
    println!("{}", "─".repeat(70));
    println!(
        "{:<28} {:>8} {:>14} {:>10} {:>8}",
        "Rule ID", "PID", "Metric", "Threshold", "Cooldown"
    );
    println!("{}", "─".repeat(70));
    for r in &rules {
        println!(
            "{:<28} {:>8} {:>14} {:>10.1} {:>8}",
            r.id, r.pid, r.metric, r.threshold, r.cooldown_secs
        );
    }
    Ok(())
}

#[cfg(unix)]
fn run_alert_remove(rule_id: &str) -> Result<()> {
    if !peekd_client::ping() {
        eprintln!(
            "{}",
            colors::yellow("peekd is not running. Start it with: sudo systemctl start peekd (or from repo: sudo mkdir -p /run/peekd && sudo ./target/release/peekd &)")
        );
        std::process::exit(1);
    }
    let removed = peekd_client::alert_remove(rule_id)?;
    if removed {
        println!("{} Removed rule {}", colors::green("✓"), colors::cyan(rule_id));
    } else {
        eprintln!("{} No rule with id '{}'", colors::yellow("⚠"), rule_id);
    }
    Ok(())
}

#[cfg(unix)]
fn run_alert_add(pid: i32, args: &[String]) -> Result<()> {
    if !peekd_client::ping() {
        eprintln!(
            "{}",
            colors::yellow("peekd is not running. Start it with: sudo systemctl start peekd (or from repo: sudo mkdir -p /run/peekd && sudo ./target/release/peekd &)")
        );
        std::process::exit(1);
    }
    let (metric, op, threshold_str) = match (args.first(), args.get(1), args.get(2)) {
        (Some(m), Some(o), Some(t)) => (m.as_str(), o.as_str(), t.as_str()),
        _ => {
            eprintln!(
                "{}",
                colors::yellow("peek --alert-add requires METRIC OP THRESHOLD (e.g. cpu_percent gt 80)")
            );
            std::process::exit(1);
        }
    };
    let threshold: f64 = threshold_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid threshold '{}', expected a number", threshold_str))?;
    let valid_metrics = ["cpu_percent", "memory_mb", "fd_count", "thread_count"];
    if !valid_metrics.contains(&metric) {
        anyhow::bail!(
            "invalid metric '{}'; choose one of: {}",
            metric,
            valid_metrics.join(", ")
        );
    }
    let rule_id = peekd_client::alert_add(pid, metric, op, threshold, "log", None)?;
    println!(
        "{} Alert rule added: {} (pid {})",
        colors::green("✓"),
        colors::cyan(&rule_id),
        pid
    );
    Ok(())
}

// ─── History (peekd) ─────────────────────────────────────────────────────────

#[cfg(not(unix))]
fn run_history(pid: i32, _cli: &Cli) -> Result<()> {
    let _ = pid;
    eprintln!(
        "{}",
        colors::yellow("peekd (history) is only available on Linux/Unix.")
    );
    std::process::exit(1);
}

#[cfg(unix)]
fn run_history(pid: i32, _cli: &Cli) -> Result<()> {
    if !peekd_client::ping() {
        eprintln!(
            "{}",
            colors::yellow("peekd is not running. Start it with: sudo systemctl start peekd (or from repo: sudo mkdir -p /run/peekd && sudo ./target/release/peekd &)")
        );
        eprintln!(
            "{}",
            colors::dimmed("Or register this PID manually: peekd watch <PID>")
        );
        std::process::exit(1);
    }

    // Register this PID for future collection
    let _ = peekd_client::register_watch(pid);

    let samples = peekd_client::fetch_history(pid)?;
    if samples.is_empty() {
        eprintln!(
            "{}",
            colors::yellow("No history yet for this PID. Wait for peekd to accumulate samples.")
        );
        return Ok(());
    }

    println!();
    println!(
        "{} — last {} samples",
        colors::bold("RESOURCE HISTORY"),
        samples.len()
    );
    println!("{}", "─".repeat(70));
    println!(
        "{:<25}  {:>8}  {:>10}  {:>10}  {:>8}",
        "Time", "CPU%", "RSS MB", "VSZ MB", "Threads"
    );
    println!("{}", "─".repeat(70));

    for s in &samples {
        println!(
            "{:<25}  {:>8}  {:>10}  {:>10}  {:>8}",
            s.ts,
            s.cpu_percent
                .map(|c| format!("{:.1}%", c))
                .unwrap_or_else(|| "-".to_string()),
            format!("{:.1}", s.rss_kb as f64 / 1024.0),
            format!("{:.1}", s.vm_size_kb as f64 / 1024.0),
            s.threads
        );
    }

    // Sparklines in terminal using block chars
    let cpu_vals: Vec<Option<f64>> = samples.iter().map(|s| s.cpu_percent).collect();
    print_terminal_sparkline("CPU %  ", &cpu_vals, 100.0);

    let rss_vals: Vec<Option<f64>> = samples
        .iter()
        .map(|s| Some(s.rss_kb as f64 / 1024.0))
        .collect();
    let rss_max = rss_vals
        .iter()
        .flatten()
        .cloned()
        .fold(0.0f64, f64::max)
        .max(1.0);
    print_terminal_sparkline("RSS MB ", &rss_vals, rss_max);

    let thread_vals: Vec<Option<f64>> =
        samples.iter().map(|s| Some(s.threads as f64)).collect();
    let threads_max = thread_vals
        .iter()
        .flatten()
        .cloned()
        .fold(0.0f64, f64::max)
        .max(1.0);
    print_terminal_sparkline("Threads", &thread_vals, threads_max);

    Ok(())
}

// ─── Port search ──────────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
fn run_port_search(port: u16) -> Result<()> {
    let _ = port;
    eprintln!(
        "{}",
        colors::yellow("Port search (--port) is only available on Linux.")
    );
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn run_port_search(port: u16) -> Result<()> {
    use network_inspector::tcp::{inodes_using_port, process_socket_inodes};
    use procfs::process::all_processes;

    println!();
    println!(
        "{} Searching for processes using TCP/UDP port {}...",
        colors::bold("🔎"),
        port
    );

    struct Hit {
        pid: i32,
        name: String,
        kind: String,
        local: String,
        remote: String,
    }

    let inode_map = inodes_using_port(port);
    let mut hits: Vec<Hit> = Vec::new();

    for pr in all_processes()?.flatten() {
        let pid = pr.pid;
        let name = pr
            .stat()
            .map(|s| s.comm.to_string())
            .unwrap_or_else(|_| pid.to_string());
        for inode in process_socket_inodes(pid) {
            if let Some((kind, local, remote)) = inode_map.get(&inode) {
                hits.push(Hit {
                    pid,
                    name: name.clone(),
                    kind: kind.clone(),
                    local: local.clone(),
                    remote: remote.clone(),
                });
            }
        }
    }

    if hits.is_empty() {
        println!("No processes found using port {}.", port);
        return Ok(());
    }

    println!();
    println!(
        "{:<6} {:<20} {:<14} {:<22} {:<22}",
        "PID", "COMMAND", "KIND", "LOCAL", "REMOTE"
    );
    println!("{}", "─".repeat(90));
    for h in &hits {
        println!(
            "{:<6} {:<20} {:<14} {:<22} {:<22}",
            h.pid,
            truncate(&h.name, 20),
            h.kind,
            h.local,
            h.remote
        );
    }

    println!();
    println!(
        "Enter a PID from above to open the interactive kill/control panel, or press Enter to exit."
    );
    print!("PID to control: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let pid: i32 = trimmed
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid PID '{}'", trimmed))?;

    let info = collect_extended(
        pid,
        &CollectOptions {
            resources: true,
            kernel: true,
            network: true,
            files: true,
            env: true,
            tree: true,
            gpu: true,
        },
    )
    .map_err(map_core_error)?;

    run_kill_panel(pid, &info)
}

fn print_terminal_sparkline(label: &str, vals: &[Option<f64>], max: f64) {
    const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bar: String = vals
        .iter()
        .map(|v| {
            let f = v.unwrap_or(0.0) / max;
            let idx = (f.clamp(0.0, 1.0) * 8.0) as usize;
            BLOCKS[idx.min(8)]
        })
        .collect();
    println!("{} |{}|", label, bar);
}

// ─── Kill panel ───────────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
fn run_kill_panel(_pid: i32, _info: &ProcessInfo) -> Result<()> {
    eprintln!(
        "{}",
        colors::yellow("Interactive kill/signal panel is only available on Linux.")
    );
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn run_kill_panel(pid: i32, info: &ProcessInfo) -> Result<()> {
    // Pre-flight impact analysis
    let impact = signal_impact(pid).ok();

    println!();
    println!("{}", colors::yellow_bold("⚡ PROCESS CONTROL"));
    println!("{}", "─".repeat(66));
    println!(
        "  Target: {} {} (pid {})",
        colors::yellow("▶"),
        colors::cyan_bold(&info.name),
        pid
    );

    // Show impact analysis
    if let Some(ref imp) = impact {
        println!();
        if imp.active_tcp_connections > 0 {
            println!(
                "  {} {} active TCP connection(s) will be affected.",
                colors::yellow("⚠"),
                imp.active_tcp_connections
            );
        }
        if imp.child_process_count > 0 {
            println!(
                "  {} {} child process(es) will be orphaned/killed.",
                colors::yellow("⚠"),
                imp.child_process_count
            );
        }
        if imp.has_file_locks {
            println!("  {} Process holds exclusive file lock(s).", colors::yellow("⚠"));
        }
        if let Some(ref unit) = imp.systemd_unit {
            println!("  {} Managed by systemd unit: {}", colors::cyan("ℹ"), colors::bold(unit));
        }
        if !imp.recommendation.is_empty() {
            println!();
            println!("  {}: {}", colors::bold("Recommendation"), imp.recommendation);
        }
    }

    // Signal menu: descriptions from signal-engine; nix::Signal from number
    println!();
    println!("{}", colors::bold("  Signals:"));
    let menu: Vec<(usize, Signal, &str, &str, i32)> = SIGNAL_MENU
        .iter()
        .enumerate()
        .filter_map(|(idx, (name, num, desc))| {
            let sig = Signal::try_from(*num).ok()?;
            Some((idx + 1, sig, *name, *desc, *num))
        })
        .collect();
    for (idx, _sig, name, desc, num) in &menu {
        println!("  [{idx}] {name:<8} ({num:>2})  {desc}");
    }

    // Systemd shortcuts
    if let Some(Some(ref unit)) = impact.as_ref().map(|i| i.systemd_unit.as_ref().cloned()) {
        println!();
        println!("  [s] systemctl stop {}", colors::cyan(unit));
        println!("  [R] systemctl restart {}", colors::cyan(unit));
    }

    println!();
    println!("  [q] Quit (do nothing)");
    println!();

    loop {
        print!("  Enter choice: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();

        match choice {
            "s" => {
                if let Some(Some(unit)) = impact.as_ref().map(|i| i.systemd_unit.as_ref().cloned())
                {
                    run_systemctl("stop", &unit)?;
                    return Ok(());
                }
                println!("  No systemd unit detected.");
            }
            "R" => {
                if let Some(Some(unit)) = impact.as_ref().map(|i| i.systemd_unit.as_ref().cloned())
                {
                    run_systemctl("restart", &unit)?;
                    return Ok(());
                }
                println!("  No systemd unit detected.");
            }
            "q" | "Q" | "" => {
                println!("  Aborted.");
                return Ok(());
            }
            other => {
                if let Ok(idx) = other.parse::<usize>() {
                    if let Some((_, sig, _name, _desc, _num)) =
                        menu.iter().find(|(i, _, _, _, _)| *i == idx)
                    {
                        let require_confirm = matches!(sig, Signal::SIGKILL);
                        return send_signal(pid, *sig, require_confirm);
                    }
                }
                println!("  Unknown choice, try again.");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn send_signal(pid: i32, sig: Signal, require_confirm: bool) -> Result<()> {
    if require_confirm {
        print!(
            "  ⚠️  Are you sure you want to FORCE KILL pid {}? [y/N]: ",
            pid
        );
        io::stdout().flush()?;
        let mut confirm = String::new();
        io::stdin().read_line(&mut confirm)?;
        if confirm.trim().to_lowercase() != "y" {
            println!("  Aborted.");
            return Ok(());
        }
    }
    match kill(Pid::from_raw(pid), sig) {
        Ok(()) => println!("  {} Sent {:?} to pid {}", colors::green("✓"), sig, pid),
        Err(e) => eprintln!("  {} Failed to send signal: {}", colors::red("✗"), e),
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_systemctl(action: &str, unit: &str) -> Result<()> {
    println!("  Running: systemctl {} {}", action, unit);
    let status = std::process::Command::new("systemctl")
        .args([action, unit])
        .status()?;
    if status.success() {
        println!("  {} systemctl {} {}", colors::green("✓"), action, unit);
    } else {
        eprintln!(
            "  {} systemctl {} {} exited with status {:?}",
            colors::red("✗"),
            action,
            unit,
            status.code()
        );
    }
    Ok(())
}

// ─── Export ───────────────────────────────────────────────────────────────────

fn run_export(snapshot: &ProcessSnapshot, format: &str) -> Result<()> {
    match format.to_lowercase().as_str() {
        "json" => println!("{}", to_json(snapshot)?),
        "md" | "markdown" => println!("{}", render_markdown(snapshot)),
        "html" => println!("{}", render_html(snapshot)),
        "pdf" => {
            let filename = export_pdf(snapshot)?;
            println!("{} PDF report written to {}", colors::green("✓"), colors::cyan(&filename));
        }
        other => anyhow::bail!("unknown format '{}' (json | html | md | pdf)", other),
    }
    Ok(())
}

// ─── Plain-text report ───────────────────────────────────────────────────────
fn print_report(info: &ProcessInfo, cli: &Cli) {
    println!();
    println!(
        "🔍 {} {}  {}",
        colors::bold("PROCESS:"),
        colors::cyan_bold(&info.name),
        colors::dimmed(&format!("(PID {})", info.pid))
    );
    println!("{}", "─".repeat(64));
    if let Some(desc) = peek_core::binary_description(&info.name) {
        println!("  {} {}", colors::dimmed("desc    :"), desc);
    }
    println!("  {} {}", colors::dimmed("cmdline :"), info.cmdline);
    if let Some(exe) = &info.exe {
        println!("  {} {}", colors::dimmed("exe     :"), exe);
    }
    println!("  {} {}", colors::dimmed("state   :"), colorize_state(&info.state));
    println!("  {} {}", colors::dimmed("ppid    :"), info.ppid);
    println!("  {} {}:{}", colors::dimmed("uid:gid :"), info.uid, info.gid);
    if let Some(started) = info.started_at {
        let age = age_string(started);
        println!(
            "  {} {} ({})",
            colors::dimmed("started :"),
            started.format("%Y-%m-%d %H:%M:%S"),
            age
        );
    }
    let mem_extra = match (info.pss_kb, info.swap_kb) {
        (Some(p), Some(s)) if s > 0 => format!("  |  {} KB PSS  |  {} KB swap", p, s),
        (Some(p), _) => format!("  |  {} KB PSS", p),
        (_, Some(s)) if s > 0 => format!("  |  {} KB swap", s),
        _ => String::new(),
    };
    println!(
        "  {} {} KB RSS / {} KB VSZ{}",
        colors::dimmed("memory  :"),
        info.rss_kb,
        info.vm_size_kb,
        mem_extra
    );
    println!("  {} {}", colors::dimmed("threads :"), info.threads);
    if let Some(fds) = info.fd_count {
        println!("  {} {}", colors::dimmed("open fds:"), fds);
        #[cfg(target_os = "linux")]
        if let Some(limit) = soft_fd_limit(info.pid) {
            if limit > 0 {
                let ratio = fds as f64 / limit as f64;
                if ratio >= 0.8 {
                    println!(
                        "  {} FD usage near soft limit: {}/{} ({:.0}%)",
                        colors::yellow("warn   :"),
                        fds,
                        limit,
                        ratio * 100.0
                    );
                }
            }
        }
    }

    // Resources
    if cli.resources || cli.all {
        println!();
        println!("{}", colors::bold("📊 RESOURCES"));
        println!("{}", "─".repeat(64));
        if let Some(cpu) = info.cpu_percent {
            let bar = progress_bar(cpu / 100.0, 20);
            let cs = if cpu > 80.0 {
                colors::red_bold(&format!("{:.1}%", cpu))
            } else if cpu > 50.0 {
                colors::yellow(&format!("{:.1}%", cpu))
            } else {
                colors::green(&format!("{:.1}%", cpu))
            };
            println!("  CPU    {} {}", bar, cs);
        }
        let mem_pct = memory_percent(info.rss_kb);
        let mem_extra = match (info.pss_kb, info.swap_kb) {
            (Some(p), Some(s)) if s > 0 => format!("  |  PSS {} KB  |  swap {} KB", p, s),
            (Some(p), _) => format!("  |  PSS {} KB", p),
            (_, Some(s)) if s > 0 => format!("  |  swap {} KB", s),
            _ => String::new(),
        };
        println!(
            "  Memory {} {:.0} MB RSS  ({:.1}% RAM){}",
            progress_bar(mem_pct / 100.0, 20),
            info.rss_kb / 1024,
            mem_pct,
            mem_extra
        );
        if let Some(r) = info.io_read_bytes {
            println!("  Disk R  {} bytes", r);
        }
        if let Some(w) = info.io_write_bytes {
            println!("  Disk W  {} bytes", w);
        }
    }

    // GPU
    if let Some(gpus) = &info.gpu {
        if !gpus.is_empty() {
            println!();
            println!("{}", colors::bold("🖥  GPU"));
            println!("{}", "─".repeat(64));
            for g in gpus {
                println!("  #{} {} [{}]", g.index, colors::cyan(&g.name), colors::dimmed(&g.source));
                if let Some(u) = g.utilization_percent {
                    println!("    Util   {} {:.1}%", progress_bar(u / 100.0, 20), u);
                }
                if let (Some(used), Some(total)) = (g.memory_used_mb, g.memory_total_mb) {
                    println!(
                        "    VRAM   {} {:.0}/{:.0} MB",
                        progress_bar(used / total, 20),
                        used,
                        total
                    );
                }
                if let Some(pmb) = g.process_used_mb {
                    println!("    Process {} {:.0} MB (this PID)", colors::cyan("▶"), pmb);
                }
            }
        }
    }

    // Kernel
    if let Some(k) = &info.kernel {
        println!();
        println!("{}", colors::bold("🧠 KERNEL CONTEXT"));
        println!("{}", "─".repeat(64));
        println!(
            "  Scheduler  : {} | Nice: {} | Priority: {}",
            k.sched_policy, k.nice, k.priority
        );
        println!(
            "  OOM Score  : {} / 1000  (adj: {}) — {}",
            oom_colored(k.oom_score),
            k.oom_score_adj,
            peek_core::oom_description(k.oom_score)
        );
        if let Some(label) = &k.security_label {
            println!("  Security   : {}", label);
        }
        if let Some((name, desc)) = peek_core::current_syscall(info.pid) {
            println!("  Syscall    : {} — {}", name, desc);
        }
        println!("  Cgroup     : {}", k.cgroup);
        println!("  Seccomp    : {}", seccomp_label(k.seccomp));
        println!("  Cap Prm    : {}", k.cap_permitted);
        println!("  Cap Eff    : {}", k.cap_effective);
        let ns: Vec<_> = k.namespaces.iter().map(|n| n.ns_type.as_str()).collect();
        if !ns.is_empty() {
            println!("  Namespaces : {}", ns.join(", "));
        }
        if let (Some(v), Some(nv)) = (k.voluntary_ctxt_switches, k.nonvoluntary_ctxt_switches) {
            println!("  Ctx Sw     : {} voluntary, {} involuntary", v, nv);
        }
    }

    // Network
    if let Some(net) = &info.network {
        println!();
        println!("{}", colors::bold("🌐 NETWORK"));
        println!("{}", "─".repeat(64));
        if let (Some(rx), Some(tx)) = (net.traffic_rx_bytes_per_sec, net.traffic_tx_bytes_per_sec) {
            println!(
                "  {} Traffic: RX {}  TX {}",
                colors::cyan("▶"),
                format_bytes_per_sec(rx),
                format_bytes_per_sec(tx)
            );
        }
        let has_listen = !net.listening_tcp.is_empty() || !net.listening_udp.is_empty();
        let has_unix = net.unix_sockets.as_ref().is_some_and(|u| !u.is_empty());
        if !has_listen
            && net.connections.is_empty()
            && !has_unix
            && net.traffic_rx_bytes_per_sec.is_none()
        {
            println!("  No sockets.");
        } else {
            for s in &net.listening_tcp {
                println!(
                    "  {} Listening (TCP): {} {}:{}",
                    colors::green("▶"),
                    s.protocol,
                    s.local_addr,
                    s.local_port
                );
            }
            for s in &net.listening_udp {
                println!(
                    "  {} Listening (UDP): {} {}:{}",
                    colors::green("▶"),
                    s.protocol,
                    s.local_addr,
                    s.local_port
                );
            }
            if let Some(unix) = &net.unix_sockets {
                if !unix.is_empty() {
                    println!("  {} Unix sockets ({}):", colors::cyan("▶"), unix.len());
                    for u in unix.iter().take(15) {
                        let path = if u.path.is_empty() {
                            "<anonymous>"
                        } else {
                            &u.path
                        };
                        println!("    {}", path);
                    }
                    if unix.len() > 15 {
                        println!("    … and {} more", unix.len() - 15);
                    }
                }
            }
            if !net.connections.is_empty() {
                println!(
                    "  {} Connections ({}):",
                    colors::yellow("▶"),
                    net.connections.len()
                );
                for c in net.connections.iter().take(20) {
                    let remote = format!("{}:{}", c.remote_addr, c.remote_port);
                    let remote_display = if cli.resolve {
                        peek_core::resolve_remote(&remote)
                            .map(|h| format!("{} ({})", h, remote))
                            .unwrap_or(remote)
                    } else {
                        remote
                    };
                    println!(
                        "    {} {}:{} → {} [{}]",
                        c.protocol,
                        c.local_addr,
                        c.local_port,
                        remote_display,
                        colors::dimmed(&c.state)
                    );
                }
                if net.connections.len() > 20 {
                    println!("    … and {} more", net.connections.len() - 20);
                }
            }
        }
    }

    // Files
    if let Some(files) = &info.open_files {
        println!();
        println!("{}", colors::bold("📁 OPEN FILES"));
        println!("{}", "─".repeat(64));
        println!("  {:>4}  {:>10}  Path", "FD", "Type");
        println!("  {}", "─".repeat(58));
        for f in files.iter().take(50) {
            println!(
                "  {:>4}  {:>10}  {}",
                f.fd,
                colors::dimmed(&f.fd_type),
                f.description
            );
        }
        if files.len() > 50 {
            println!("  … {} more", files.len() - 50);
        }
        println!("  Total: {}", files.len());
    }

    // Env
    if let Some(env_vars) = &info.env_vars {
        println!();
        println!("{}", colors::bold("🔐 ENVIRONMENT"));
        println!("{}", "─".repeat(64));
        let secrets = env_vars.iter().filter(|v| v.redacted).count();
        if secrets > 0 {
            println!(
                "  {} {} secret(s) detected and redacted.",
                colors::yellow("⚠️"),
                secrets
            );
        }
        let max_k = env_vars
            .iter()
            .map(|v| v.key.len())
            .max()
            .unwrap_or(10)
            .min(40);
        for v in env_vars {
            let ks = if v.redacted {
                colors::yellow(&v.key)
            } else {
                colors::cyan(&v.key)
            };
            let vs = if v.redacted {
                format!("{} {}", v.value, colors::red("[REDACTED]"))
            } else {
                v.value.chars().take(80).collect()
            };
            println!("  {:<width$} = {}", ks, vs, width = max_k + 2);
        }
    }

    // Tree
    if let Some(tree) = &info.process_tree {
        println!();
        println!("{}", colors::bold("🌳 PROCESS TREE"));
        println!("{}", "─".repeat(64));
        print_tree(tree, "", true, info.pid);
    }

    println!();
}

fn print_tree(node: &ProcessNode, prefix: &str, is_last: bool, target: i32) {
    let conn = if is_last { "└── " } else { "├── " };
    let name = if node.pid == target {
        colors::cyan_bold(&node.name)
    } else {
        node.name.clone()
    };
    println!(
        "  {}{}{} ({}) [{} MB]",
        prefix,
        conn,
        name,
        node.pid,
        node.rss_kb / 1024
    );
    let child_pfx = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    for (i, child) in node.children.iter().enumerate() {
        print_tree(child, &child_pfx, i == node.children.len() - 1, target);
    }
}

// ─── Utilities ───────────────────────────────────────────────────────────────

fn map_core_error(err: PeekError) -> anyhow::Error {
    match err {
        PeekError::NotFound(pid) => anyhow::anyhow!("process {} not found", pid),
        PeekError::ProcIo { pid, source } if source.kind() == io::ErrorKind::PermissionDenied => {
            anyhow::anyhow!(
                "permission denied reading /proc/{} (try --sudo or run as root): {}",
                pid,
                source
            )
        }
        other => anyhow::anyhow!(other),
    }
}

/// Map a high-level error into a process exit code suitable for scripting.
///
/// Currently:
/// - 3 → process not found
/// - 4 → permission denied (/proc, IO)
/// - 1 → all other errors
fn exit_code_for_error(err: &anyhow::Error) -> i32 {
    use std::io::ErrorKind;

    if let Some(peek) = err.downcast_ref::<PeekError>() {
        return match peek {
            PeekError::NotFound(_) => 3,
            PeekError::ProcIo { source, .. } if source.kind() == ErrorKind::PermissionDenied => 4,
            _ => 1,
        };
    }

    if let Some(io_err) = err.downcast_ref::<io::Error>() {
        if io_err.kind() == ErrorKind::PermissionDenied {
            return 4;
        }
    }

    1
}

fn resolve_target(cli: &Cli) -> Result<i32> {
    if let Some(ref t) = cli.target {
        if let Ok(pid) = t.parse::<i32>() {
            if pid > 0 {
                return Ok(pid);
            }
        }
        return resolve_by_name(t);
    }

    // No explicit target: interactive mode — pick a process (Linux only).
    #[cfg(target_os = "linux")]
    return interactive_select_pid();

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!(
            "{}",
            colors::yellow("peek: no target given. Usage: peek <PID|name> [options]. On Linux, run peek with no args for interactive process picker.")
        );
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn resolve_by_name(name: &str) -> Result<i32> {
    use procfs::process::all_processes;
    let mut matches = Vec::new();
    for pr in all_processes()?.flatten() {
        if let Ok(stat) = pr.stat() {
            if stat.comm == name {
                matches.push(pr.pid);
                continue;
            }
        }
        if let Ok(cmdline) = pr.cmdline() {
            if cmdline.join(" ").contains(name) {
                matches.push(pr.pid);
            }
        }
    }
    match matches.len() {
        0 => anyhow::bail!("no process matching '{}'", name),
        1 => Ok(matches[0]),
        _ => anyhow::bail!(
            "multiple matches for '{}': {:?}\nSpecify a PID.",
            name,
            &matches[..matches.len().min(5)]
        ),
    }
}

#[cfg(target_os = "linux")]
fn interactive_select_pid() -> Result<i32> {
    use procfs::process::all_processes;

    #[derive(Debug)]
    struct Row {
        pid: i32,
        name: String,
        cmd: String,
    }

    let mut rows = Vec::new();
    for pr in all_processes()?.flatten() {
        let pid = pr.pid;
        let stat_name = pr
            .stat()
            .ok()
            .map(|s| s.comm)
            .unwrap_or_else(|| "?".to_string());
        let cmdline = pr
            .cmdline()
            .ok()
            .filter(|c| !c.is_empty())
            .map(|c| c.join(" "))
            .unwrap_or_else(|| stat_name.clone());
        rows.push(Row {
            pid,
            name: stat_name,
            cmd: cmdline,
        });
    }

    if rows.is_empty() {
        anyhow::bail!("no processes found");
    }

    // Sort by PID ascending for a stable view.
    rows.sort_by_key(|r| r.pid);

    let mut filter: Option<String> = None;

    loop {
        println!();
        println!("{}", colors::bold("Interactive process browser"));
        println!("{}", "─".repeat(64));

        let filtered: Vec<(usize, &Row)> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                if let Some(ref f) = filter {
                    let f = f.to_lowercase();
                    r.name.to_lowercase().contains(&f) || r.cmd.to_lowercase().contains(&f)
                } else {
                    true
                }
            })
            .take(30)
            .collect();

        println!(
            "{:<4} {:>6}  {:<20}  Command (truncated)",
            "#", "PID", "Name"
        );
        println!("{}", "─".repeat(64));
        for (idx, row) in &filtered {
            let cmd_trunc: String = if row.cmd.len() > 40 {
                format!("{}…", &row.cmd[..40])
            } else {
                row.cmd.clone()
            };
            println!("{:<4} {:>6}  {:<20}  {}", idx + 1, row.pid, row.name, cmd_trunc);
        }

        println!();
        println!(
            "{}",
            colors::dimmed(
                "Enter index, PID, or search term (/filter). Empty input to cancel, q to quit."
            )
        );
        print!("peek> ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() || input.eq_ignore_ascii_case("q") {
            anyhow::bail!("no target selected");
        }

        if let Some(rest) = input.strip_prefix('/') {
            filter = if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            };
            continue;
        }

        // Try explicit PID first.
        if let Ok(pid) = input.parse::<i32>() {
            if rows.iter().any(|r| r.pid == pid) {
                return Ok(pid);
            }
        }

        // Try visible index.
        if let Ok(idx) = input.parse::<usize>() {
            if idx > 0 && idx <= filtered.len() {
                return Ok(filtered[idx - 1].1.pid);
            }
        }

        // Fuzzy match on name/cmd.
        let q = input.to_lowercase();
        let matches: Vec<&Row> = rows
            .iter()
            .filter(|r| {
                r.name.to_lowercase().contains(&q) || r.cmd.to_lowercase().contains(&q)
            })
            .collect();
        match matches.len() {
            0 => {
                println!("No processes match '{}'.", input);
            }
            1 => {
                return Ok(matches[0].pid);
            }
            _ => {
                filter = Some(q);
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn resolve_by_name(name: &str) -> Result<i32> {
    use sysinfo::{Pid, System};
    let mut sys = System::new_all();
    sys.refresh_processes();
    let name_lower = name.to_lowercase();
    let mut matches: Vec<i32> = Vec::new();
    for (pid, process) in sys.processes() {
        if process
            .name()
            .to_string_lossy()
            .to_lowercase()
            .contains(&name_lower)
        {
            matches.push(pid.as_u32() as i32);
            continue;
        }
        let cmd = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<String>();
        if cmd.to_lowercase().contains(&name_lower) {
            matches.push(pid.as_u32() as i32);
        }
    }
    match matches.len() {
        0 => anyhow::bail!("no process matching '{}'", name),
        1 => Ok(matches[0]),
        _ => anyhow::bail!(
            "multiple matches for '{}': {:?}\nSpecify a PID.",
            name,
            &matches[..matches.len().min(5)]
        ),
    }
}

fn colorize_state(s: &str) -> String {
    if s.starts_with("Running") {
        colors::green(s)
    } else if s.starts_with("Zombie") | s.starts_with("Dead") {
        colors::red_bold(s)
    } else if s.starts_with("Uninterruptible") {
        colors::yellow(s)
    } else {
        s.to_string()
    }
}

fn oom_colored(score: i32) -> String {
    let s = score.to_string();
    if score > 700 {
        colors::red_bold(&s)
    } else if score > 400 {
        colors::yellow(&s)
    } else {
        colors::green(&s)
    }
}

fn seccomp_label(v: u32) -> &'static str {
    match v {
        0 => "disabled",
        1 => "strict",
        2 => "filter active",
        _ => "unknown",
    }
}

fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = (fraction.clamp(0.0, 1.0) * width as f64).round() as usize;
    format!(
        "[{}{}]",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

/// RAM usage as % of total. Linux: /proc/meminfo; other platforms: sysinfo.
#[cfg(target_os = "linux")]
fn memory_percent(rss_kb: u64) -> f64 {
    let total = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(1);
    if total == 0 {
        return 0.0;
    }
    (rss_kb as f64 / total as f64) * 100.0
}

#[cfg(not(target_os = "linux"))]
fn memory_percent(rss_kb: u64) -> f64 {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    let total_kb = sys.total_memory() / 1024;
    if total_kb == 0 {
        return 0.0;
    }
    (rss_kb as f64 / total_kb as f64) * 100.0
}

fn format_bytes_per_sec(b: u64) -> String {
    if b >= 1_000_000 {
        format!("{:.1} MB/s", b as f64 / 1_000_000.0)
    } else if b >= 1000 {
        format!("{:.1} KB/s", b as f64 / 1000.0)
    } else {
        format!("{} B/s", b)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i + 1 >= max {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(target_os = "linux")]
fn soft_fd_limit(pid: i32) -> Option<u64> {
    peek_core::fd_soft_limit(pid)
}

fn age_string(started: chrono::DateTime<chrono::Local>) -> String {
    let s = chrono::Local::now()
        .signed_duration_since(started)
        .num_seconds();
    if s < 60 {
        format!("{}s ago", s)
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86400 {
        format!("{}h ago", s / 3600)
    } else {
        format!("{}d ago", s / 86400)
    }
}
