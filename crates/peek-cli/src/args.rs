use clap::Parser;

/// Process intelligence tool for Linux.
///
/// Inspect and troubleshoot a single process by PID or name, view live
/// resource usage, network connections, open files, kernel context, GPU,
/// and more. You can also search for processes by TCP/UDP port using
/// `--port <PORT>` and then open an interactive kill/control panel.
#[derive(Debug, Clone, Parser)]
#[command(name = "peek", version)]
pub struct Cli {
    /// PID or process name to inspect
    pub target: Option<String>,

    /// Show resource usage dashboard
    #[arg(short = 'r', long)]
    pub resources: bool,

    /// Show kernel context (scheduler, OOM, namespaces, seccomp)
    #[arg(short = 'k', long)]
    pub kernel: bool,

    /// Show network connections and ports
    #[arg(short = 'n', long)]
    pub network: bool,

    /// Resolve remote addresses to hostnames (best-effort, time-bounded)
    #[arg(long)]
    pub resolve: bool,

    /// List open file descriptors
    #[arg(short = 'f', long)]
    pub files: bool,

    /// Show environment variables (secrets redacted)
    #[arg(short = 'e', long)]
    pub env: bool,

    /// Show full process tree
    #[arg(short = 't', long)]
    pub tree: bool,

    /// Live-updating mode (default: 2000ms refresh). Optionally pass interval in ms.
    #[arg(
        short = 'w',
        long,
        value_name = "INTERVAL_MS",
        conflicts_with_all = [
            "json",
            "json_snapshot",
            "export",
            "kill",
            "history",
            "alert_list",
            "alert_add",
            "alert_remove",
            "diff"
        ]
    )]
    pub watch: Option<Option<u64>>,

    /// Interactive kill/control panel
    #[arg(long)]
    pub kill: bool,

    /// Show everything
    #[arg(short = 'a', long)]
    pub all: bool,

    /// Export report format: json | html | md
    #[arg(
        long,
        value_name = "FORMAT",
        conflicts_with_all = [
            "watch",
            "json",
            "json_snapshot",
            "kill",
            "history",
            "alert_list",
            "alert_add",
            "alert_remove",
            "diff"
        ]
    )]
    pub export: Option<String>,

    /// Raw JSON output (suppress interactive UI)
    #[arg(
        short = 'j',
        long,
        conflicts_with_all = [
            "watch",
            "export",
            "kill",
            "history",
            "alert_list",
            "alert_add",
            "alert_remove",
            "diff"
        ]
    )]
    pub json: bool,

    /// JSON snapshot (captured_at, peek_version, process)
    #[arg(
        long,
        conflicts_with_all = [
            "watch",
            "export",
            "kill",
            "history",
            "alert_list",
            "alert_add",
            "alert_remove",
            "diff"
        ]
    )]
    pub json_snapshot: bool,

    /// Disable colour output
    #[arg(long)]
    pub no_color: bool,

    /// Compare with another process
    #[arg(long, value_name = "PID2")]
    pub diff: Option<i32>,

    /// Show resource history (requires peekd daemon)
    #[arg(long)]
    pub history: bool,

    /// List alert rules (requires peekd)
    #[arg(long)]
    pub alert_list: bool,

    /// Add alert rule: METRIC GT|LT THRESHOLD (e.g. cpu_percent gt 80). Requires target PID.
    #[arg(long, value_name = "METRIC OP THRESHOLD", num_args = 3)]
    pub alert_add: Option<Vec<String>>,

    /// Remove alert rule by rule_id (from --alert-list)
    #[arg(long, value_name = "RULE_ID")]
    pub alert_remove: Option<String>,

    /// Request elevated privileges via sudo
    #[arg(long)]
    pub sudo: bool,

    /// Enable verbose diagnostic output on errors
    #[arg(long)]
    pub verbose: bool,

    /// Network traffic sample window in milliseconds (0 to skip rate sampling)
    #[arg(long, value_name = "MILLISECONDS")]
    pub net_sample_ms: Option<u64>,

    /// Search for processes listening on or connected to a TCP/UDP PORT
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_basic_target_and_flags() {
        let cli = Cli::parse_from(["peek", "1234", "--resources", "--no-color"]);
        assert_eq!(cli.target.as_deref(), Some("1234"));
        assert!(cli.resources);
        assert!(cli.no_color);
    }

    #[test]
    fn parses_export_format_and_history() {
        let cli = Cli::parse_from(["peek", "nginx", "--export", "md"]);
        assert_eq!(cli.target.as_deref(), Some("nginx"));
        assert_eq!(cli.export.as_deref(), Some("md"));
    }
}
