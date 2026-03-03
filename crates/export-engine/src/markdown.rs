// Render report as Markdown. Logic moved from peek-cli `render_markdown`.

use crate::snapshot::ProcessSnapshot;

/// Render a process snapshot as a Markdown report (includes capture time and peek version).
pub fn render_markdown(snapshot: &ProcessSnapshot) -> String {
    let info = &snapshot.process;
    let mut out = String::new();
    out.push_str(&format!(
        "# peek report — {} (PID {})\n\n",
        info.name, info.pid
    ));
    out.push_str(&format!(
        "> Generated {} (peek {})\n\n",
        snapshot.captured_at.format("%Y-%m-%d %H:%M:%S UTC"),
        snapshot.peek_version
    ));

    out.push_str("## Process\n\n| Field | Value |\n|---|---|\n");
    out.push_str(&format!("| Name | `{}` |\n", info.name));
    out.push_str(&format!("| PID | {} |\n", info.pid));
    out.push_str(&format!("| PPID | {} |\n", info.ppid));
    if let Some(exe) = &info.exe {
        out.push_str(&format!("| Exe | `{}` |\n", exe));
    }
    out.push_str(&format!("| Command | `{}` |\n", info.cmdline));
    out.push_str(&format!("| State | {} |\n", info.state));
    out.push_str(&format!("| UID:GID | {}:{} |\n", info.uid, info.gid));
    if let Some(s) = info.started_at {
        out.push_str(&format!("| Started | {} |\n", s));
    }
    out.push_str(&format!("| Threads | {} |\n", info.threads));
    out.push_str(&format!("| RSS KB | {} |\n", info.rss_kb));
    if let Some(p) = info.pss_kb {
        out.push_str(&format!("| PSS KB | {} |\n", p));
    }
    if let Some(s) = info.swap_kb {
        out.push_str(&format!("| Swap KB | {} |\n", s));
    }
    out.push_str(&format!("| VSZ KB | {} |\n", info.vm_size_kb));

    if let Some(cpu) = info.cpu_percent {
        out.push_str("\n## Resources\n\n| Field | Value |\n|---|---|\n");
        out.push_str(&format!("| CPU % | {:.1} |\n", cpu));
        out.push_str(&format!("| RSS KB | {} |\n", info.rss_kb));
        if let Some(p) = info.pss_kb {
            out.push_str(&format!("| PSS KB | {} |\n", p));
        }
        if let Some(s) = info.swap_kb {
            out.push_str(&format!("| Swap KB | {} |\n", s));
        }
        if let Some(r) = info.io_read_bytes {
            out.push_str(&format!("| Disk read | {} B |\n", r));
        }
        if let Some(w) = info.io_write_bytes {
            out.push_str(&format!("| Disk write | {} B |\n", w));
        }
        if let Some(f) = info.fd_count {
            out.push_str(&format!("| Open FDs | {} |\n", f));
        }
    }

    if let Some(k) = &info.kernel {
        out.push_str("\n## Kernel\n\n| Field | Value |\n|---|---|\n");
        out.push_str(&format!("| Scheduler | {} |\n", k.sched_policy));
        out.push_str(&format!(
            "| Nice / Priority | {} / {} |\n",
            k.nice, k.priority
        ));
        out.push_str(&format!("| OOM Score | {} / 1000 |\n", k.oom_score));
        out.push_str(&format!("| OOM Adj | {} |\n", k.oom_score_adj));
        out.push_str(&format!("| Cgroup | `{}` |\n", k.cgroup));
        out.push_str(&format!("| Seccomp | {} |\n", k.seccomp));
        out.push_str(&format!("| Cap Permitted | {} |\n", k.cap_permitted));
        out.push_str(&format!("| Cap Effective | {} |\n", k.cap_effective));
    }

    if let Some(net) = &info.network {
        out.push_str("\n## Network\n\n");
        if let (Some(rx), Some(tx)) = (net.traffic_rx_bytes_per_sec, net.traffic_tx_bytes_per_sec) {
            out.push_str(&format!(
                "**Traffic:** RX {} / s, TX {} / s\n\n",
                format_bytes_per_sec_md(rx),
                format_bytes_per_sec_md(tx)
            ));
        }
        if !net.listening_tcp.is_empty() {
            out.push_str("**Listening (TCP):**\n\n");
            for s in &net.listening_tcp {
                out.push_str(&format!(
                    "- `{}` {}:{}\n",
                    s.protocol, s.local_addr, s.local_port
                ));
            }
        }
        if !net.listening_udp.is_empty() {
            out.push_str("**Listening (UDP):**\n\n");
            for s in &net.listening_udp {
                out.push_str(&format!(
                    "- `{}` {}:{}\n",
                    s.protocol, s.local_addr, s.local_port
                ));
            }
        }
        if let Some(unix) = &net.unix_sockets {
            if !unix.is_empty() {
                out.push_str("**Unix sockets:**\n\n");
                for u in unix.iter().take(20) {
                    let path = if u.path.is_empty() {
                        "<anonymous>"
                    } else {
                        u.path.as_str()
                    };
                    out.push_str(&format!("- `{}`\n", path));
                }
            }
        }
        if !net.connections.is_empty() {
            out.push_str(&format!(
                "\n**Connections ({}):**\n\n",
                net.connections.len()
            ));
            out.push_str("| Proto | Local | Remote | State |\n|---|---|---|---|\n");
            for c in net.connections.iter().take(30) {
                out.push_str(&format!(
                    "| {} | {}:{} | {}:{} | {} |\n",
                    c.protocol, c.local_addr, c.local_port, c.remote_addr, c.remote_port, c.state
                ));
            }
        }
    }

    if let Some(files) = &info.open_files {
        out.push_str(&format!("\n## Open Files ({} total)\n\n", files.len()));
        out.push_str("| FD | Type | Path |\n|---|---|---|\n");
        for f in files.iter().take(50) {
            out.push_str(&format!(
                "| {} | {} | `{}` |\n",
                f.fd, f.fd_type, f.description
            ));
        }
    }

    if let Some(env_vars) = &info.env_vars {
        let secrets = env_vars.iter().filter(|v| v.redacted).count();
        out.push_str(&format!(
            "\n## Environment ({} vars, {} redacted)\n\n",
            env_vars.len(),
            secrets
        ));
        out.push_str("| Key | Value |\n|---|---|\n");
        for v in env_vars {
            out.push_str(&format!("| `{}` | {} |\n", v.key, v.value));
        }
    }

    if let Some(gpus) = &info.gpu {
        if !gpus.is_empty() {
            out.push_str(
                "\n## GPU\n\n| Index | Name | Util% | Mem Used | Mem Total | Process (MB) |\n|---|---|---|---|---|---|\n",
            );
            for g in gpus {
                let process_mb = g
                    .process_used_mb
                    .map(|p| format!("{:.0}", p))
                    .unwrap_or_else(|| "-".to_string());
                out.push_str(&format!(
                    "| {} | {} | {:.1} | {:.0} MB | {:.0} MB | {} |\n",
                    g.index,
                    g.name,
                    g.utilization_percent.unwrap_or(0.0),
                    g.memory_used_mb.unwrap_or(0.0),
                    g.memory_total_mb.unwrap_or(0.0),
                    process_mb,
                ));
            }
        }
    }

    if let Some(tree) = &info.process_tree {
        out.push_str("\n## Process Tree\n\n```text\n");
        append_process_tree_md(&mut out, tree, "", true, info.pid);
        out.push_str("```\n");
    }

    out
}

fn append_process_tree_md(
    out: &mut String,
    node: &peek_core::ProcessNode,
    prefix: &str,
    is_last: bool,
    target: i32,
) {
    let conn = if is_last { "└── " } else { "├── " };
    let name = if node.pid == target {
        format!("{} (this)", node.name)
    } else {
        node.name.clone()
    };
    out.push_str(&format!(
        "  {}{}{} ({}) [{} MB]\n",
        prefix,
        conn,
        name,
        node.pid,
        node.rss_kb / 1024
    ));
    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    for (i, child) in node.children.iter().enumerate() {
        append_process_tree_md(out, child, &child_prefix, i == node.children.len() - 1, target);
    }
}

fn format_bytes_per_sec_md(b: u64) -> String {
    if b >= 1_000_000 {
        format!("{:.1} MB/s", b as f64 / 1_000_000.0)
    } else if b >= 1000 {
        format!("{:.1} KB/s", b as f64 / 1000.0)
    } else {
        format!("{} B/s", b)
    }
}

#[cfg(test)]
mod tests {
    use super::render_markdown;
    use crate::snapshot::ProcessSnapshot;
    use chrono::{TimeZone, Utc};
    use peek_core::ProcessInfo;

    #[test]
    fn markdown_includes_basic_sections() {
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

        let md = render_markdown(&snapshot);
        assert!(md.contains("## Process"));
        assert!(md.contains("peek report"));
    }
}
