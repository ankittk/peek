use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table, Tabs, Wrap,
    },
    Frame,
};

use super::app::{App, FdLeakStatus, TABS};
use peek_core::ProcessNode;

// ─── Main draw entry point ────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Outer layout: header bar | tab bar | content | footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(2), // tabs
            Constraint::Min(0),    // content
            Constraint::Length(1), // footer
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_tabs(f, app, chunks[1]);
    draw_content(f, app, chunks[2]);
    draw_footer(f, app, chunks[3]);

    // Optional help overlay drawn on top of the main UI
    if app.show_help {
        draw_help_overlay(f, app, area);
    }
}

// ─── Header ───────────────────────────────────────────────────────────────────

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let info = match &app.info {
        Some(i) => i,
        None => {
            let p = Paragraph::new("Loading…").block(Block::default().borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let state_color = match info.state.as_str() {
        s if s.starts_with("Running") => Color::Green,
        s if s.starts_with("Zombie") | s.starts_with("Dead") => Color::Red,
        s if s.starts_with("Uninterruptible") => Color::Yellow,
        _ => Color::White,
    };

    let age = info
        .started_at
        .map(|t| {
            let secs = chrono::Local::now().signed_duration_since(t).num_seconds();
            if secs < 3600 {
                format!("{}m", secs / 60)
            } else if secs < 86400 {
                format!("{}h", secs / 3600)
            } else {
                format!("{}d", secs / 86400)
            }
        })
        .unwrap_or_else(|| "?".to_string());

    let fd_warn = matches!(app.fd_leak, FdLeakStatus::Warning { .. });

    let title_spans = vec![
        Span::raw(" 🔍 "),
        Span::styled(
            &info.name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  PID {}", info.pid)),
        Span::raw("  │  "),
        Span::styled(&info.state, Style::default().fg(state_color)),
        Span::raw(format!("  │  up {}", age)),
        Span::raw(format!("  │  uid:{}  threads:{}", info.uid, info.threads)),
        if fd_warn {
            Span::styled(
                "  ⚠ FD LEAK",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
    ];

    if app.paused {
        let p = Paragraph::new(Line::from(vec![
            Span::styled(" ⏸ PAUSED — ", Style::default().fg(Color::Yellow)),
            Span::raw(info.name.clone()),
        ]))
        .block(Block::default().borders(Borders::ALL).title("peek --watch"));
        f.render_widget(p, area);
    } else {
        let p = Paragraph::new(Line::from(title_spans))
            .block(Block::default().borders(Borders::ALL).title("peek --watch"));
        f.render_widget(p, area);
    }
}

// ─── Tab bar ─────────────────────────────────────────────────────────────────

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = TABS.iter().map(|t| Line::from(*t)).collect();
    let tabs = Tabs::new(titles)
        .select(app.active_tab)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(symbols::line::VERTICAL);
    f.render_widget(tabs, area);
}

// ─── Content router ───────────────────────────────────────────────────────────

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    if let Some(err) = &app.error {
        let p = Paragraph::new(format!("Error: {}", err))
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(p, area);
        return;
    }

    if app.info.is_none() {
        let p = Paragraph::new("Waiting for first sample…")
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(p, area);
        return;
    }

    match app.active_tab {
        0 => draw_overview(f, app, area),
        1 => draw_kernel(f, app, area),
        2 => draw_network(f, app, area),
        3 => draw_files(f, app, area),
        4 => draw_env(f, app, area),
        5 => draw_tree(f, app, area),
        6 => draw_gpu(f, app, area),
        _ => {}
    }
}

// ─── Overview tab ─────────────────────────────────────────────────────────────

pub(crate) fn draw_overview(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // CPU sparkline
            Constraint::Length(5), // RSS sparkline
            Constraint::Length(5), // FD sparkline
            Constraint::Length(4), // gauges
            Constraint::Min(0),    // info text
        ])
        .split(area);

    // CPU sparkline (data precomputed in App::refresh)
    let cpu_data = app.cpu_sparkline();
    let cpu_pct = info.cpu_percent.unwrap_or(0.0);
    let cpu_sparkline = Sparkline::default()
        .block(
            Block::default()
                .title(format!(
                    " CPU  {:.1}% (last {} samples)",
                    cpu_pct,
                    cpu_data.len()
                ))
                .borders(Borders::ALL),
        )
        .data(cpu_data)
        .max(1000)
        .style(Style::default().fg(cpu_color(cpu_pct)));
    f.render_widget(cpu_sparkline, chunks[0]);

    // RSS sparkline
    let rss_data = app.rss_sparkline();
    let rss_mb = info.rss_kb / 1024;
    let mem_title = match (info.pss_kb, info.swap_kb) {
        (Some(p), Some(s)) if s > 0 => format!(
            " Memory  {} MB RSS  /  {} MB VSZ  |  PSS {} KB  swap {} KB",
            rss_mb,
            info.vm_size_kb / 1024,
            p,
            s
        ),
        (Some(p), _) => format!(
            " Memory  {} MB RSS  /  {} MB VSZ  |  PSS {} KB",
            rss_mb,
            info.vm_size_kb / 1024,
            p
        ),
        (_, Some(s)) if s > 0 => format!(
            " Memory  {} MB RSS  /  {} MB VSZ  |  swap {} KB",
            rss_mb,
            info.vm_size_kb / 1024,
            s
        ),
        _ => format!(
            " Memory  {} MB RSS  /  {} MB VSZ",
            rss_mb,
            info.vm_size_kb / 1024
        ),
    };
        let rss_max = rss_data.iter().copied().max().unwrap_or(1).max(1);
    let rss_sparkline = Sparkline::default()
        .block(Block::default().title(mem_title).borders(Borders::ALL))
        .data(rss_data)
        .max(rss_max)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(rss_sparkline, chunks[1]);

    // FD sparkline (with leak warning coloring)
    let fd_data = app.fd_sparkline();
    let fd_cur = info.fd_count.unwrap_or(0);
    let fd_max = fd_data.iter().copied().max().unwrap_or(1).max(1);
    let fd_color = match app.fd_leak {
        FdLeakStatus::Warning { .. } => Color::Red,
        FdLeakStatus::Ok => Color::Gray,
    };
    let fd_title = match app.fd_leak {
        FdLeakStatus::Warning { start, end, n } => {
            format!(
                " FDs  {} open  ⚠ LEAK DETECTED (+{} over {} samples)",
                fd_cur,
                end - start,
                n
            )
        }
        FdLeakStatus::Ok => format!(" FDs  {} open", fd_cur),
    };
    let fd_sparkline = Sparkline::default()
        .block(Block::default().title(fd_title).borders(Borders::ALL))
        .data(fd_data)
        .max(fd_max)
        .style(Style::default().fg(fd_color));
    f.render_widget(fd_sparkline, chunks[2]);

    // Gauges row
    let gauge_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[3]);

    let cpu_pct_u16 = cpu_pct.clamp(0.0, 100.0) as u16;
    let cpu_gauge = Gauge::default()
        .block(Block::default().title(" CPU ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(cpu_color(cpu_pct)))
        .percent(cpu_pct_u16);
    f.render_widget(cpu_gauge, gauge_chunks[0]);

    let mem_pct = memory_percent(info.rss_kb);
    let mem_gauge = Gauge::default()
        .block(Block::default().title(" RAM ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(mem_color(mem_pct)))
        .percent(mem_pct.clamp(0.0, 100.0) as u16);
    f.render_widget(mem_gauge, gauge_chunks[1]);

    // Summary text
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("  cmdline : ", Style::default().fg(Color::DarkGray)),
            Span::raw(&info.cmdline),
        ]),
        Line::from(vec![
            Span::styled("  ppid    : ", Style::default().fg(Color::DarkGray)),
            Span::raw(info.ppid.to_string()),
        ]),
    ];
    if let Some(io_r) = info.io_read_bytes {
        lines.push(Line::from(format!(
            "  disk I/O: read {} bytes / write {} bytes",
            io_r,
            info.io_write_bytes.unwrap_or(0)
        )));
    }
    if let Some(gpus) = &info.gpu {
        for g in gpus {
            let process_str = g
                .process_used_mb
                .map(|p| format!("  (process: {:.0} MB)", p))
                .unwrap_or_default();
            lines.push(Line::from(format!(
                "  GPU #{} {} : {:.1}%  {:.0}/{:.0} MB{}",
                g.index,
                g.name,
                g.utilization_percent.unwrap_or(0.0),
                g.memory_used_mb.unwrap_or(0.0),
                g.memory_total_mb.unwrap_or(0.0),
                process_str,
            )));
        }
    }

    let summary = Paragraph::new(lines)
        .block(Block::default().title(" Details ").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(summary, chunks[4]);
}

// ─── Kernel tab ───────────────────────────────────────────────────────────────

pub(crate) fn draw_kernel(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let k = match &info.kernel {
        Some(k) => k,
        None => {
            let p = Paragraph::new("Run with -k / --kernel to collect kernel context.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let oom_color = if k.oom_score > 700 {
        Color::Red
    } else if k.oom_score > 400 {
        Color::Yellow
    } else {
        Color::Green
    };

    let seccomp_str = match k.seccomp {
        0 => "disabled",
        1 => "strict",
        2 => "filter active",
        _ => "unknown",
    };

    let ns_names: Vec<String> = k.namespaces.iter().map(|n| n.ns_type.clone()).collect();

    let nice_prio = format!("{} / {}", k.nice, k.priority);
    let oom_score_str = format!("{} / 1000", k.oom_score);
    let oom_adj_str = k.oom_score_adj.to_string();
    let ns_joined = ns_names.join(", ");
    let ctx_sw = format!(
        "{} voluntary / {} involuntary",
        k.voluntary_ctxt_switches.unwrap_or(0),
        k.nonvoluntary_ctxt_switches.unwrap_or(0)
    );

    let rows: Vec<Row> = vec![
        row2("Scheduler", &k.sched_policy),
        row2("Nice / Priority", &nice_prio),
        row2_colored("OOM Score", &oom_score_str, oom_color),
        row2("OOM Score Adj", &oom_adj_str),
        row2("Cgroup", &k.cgroup),
        row2("Seccomp", seccomp_str),
        row2("Namespaces", &ns_joined),
        row2("Cap Permitted", &k.cap_permitted),
        row2("Cap Effective", &k.cap_effective),
        row2("Ctx Switches", &ctx_sw),
    ];

    let table = Table::new(rows, [Constraint::Length(18), Constraint::Min(0)])
        .block(
            Block::default()
                .title(" Kernel Context ")
                .borders(Borders::ALL),
        )
        .header(
            Row::new(vec!["Field", "Value"]).style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ),
        );

    f.render_widget(table, area);
}

fn row2<'a>(key: &'a str, value: &'a str) -> Row<'a> {
    Row::new(vec![
        Cell::from(key).style(Style::default().fg(Color::DarkGray)),
        Cell::from(value.to_string()),
    ])
}

fn row2_colored<'a>(key: &'a str, value: &'a str, color: Color) -> Row<'a> {
    Row::new(vec![
        Cell::from(key).style(Style::default().fg(Color::DarkGray)),
        Cell::from(value.to_string()).style(Style::default().fg(color)),
    ])
}

// ─── Network tab ──────────────────────────────────────────────────────────────

pub(crate) fn draw_network(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let net = match &info.network {
        Some(n) => n,
        None => {
            let p = Paragraph::new("Run with -n / --network to collect network info.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let listen_count = net.listening_tcp.len() + net.listening_udp.len();
    let has_traffic =
        net.traffic_rx_bytes_per_sec.is_some() || net.traffic_tx_bytes_per_sec.is_some();
    let constraints = if has_traffic {
        vec![
            Constraint::Length(1),
            Constraint::Length(listen_count as u16 + 3),
            Constraint::Min(0),
        ]
    } else {
        vec![
            Constraint::Length(listen_count as u16 + 3),
            Constraint::Min(0),
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let (listen_chunk, table_chunk) = if has_traffic {
        (chunks[1], chunks[2])
    } else {
        (chunks[0], chunks[1])
    };

    if has_traffic {
        let (rx, tx) = (
            net.traffic_rx_bytes_per_sec.unwrap_or(0),
            net.traffic_tx_bytes_per_sec.unwrap_or(0),
        );
        let rx_s = if rx >= 1_000_000 {
            format!("{:.1} MB/s", rx as f64 / 1_000_000.0)
        } else if rx >= 1000 {
            format!("{:.1} KB/s", rx as f64 / 1000.0)
        } else {
            format!("{} B/s", rx)
        };
        let tx_s = if tx >= 1_000_000 {
            format!("{:.1} MB/s", tx as f64 / 1_000_000.0)
        } else if tx >= 1000 {
            format!("{:.1} KB/s", tx as f64 / 1000.0)
        } else {
            format!("{} B/s", tx)
        };
        let traffic_line = Paragraph::new(format!("  Traffic  RX {}   TX {}", rx_s, tx_s))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(traffic_line, chunks[0]);
    }

    // Listening ports (TCP then UDP)
    let mut listen_items: Vec<ListItem> = net
        .listening_tcp
        .iter()
        .map(|s| ListItem::new(format!("  TCP  {}:{}", s.local_addr, s.local_port)))
        .collect();
    listen_items.extend(
        net.listening_udp
            .iter()
            .map(|s| ListItem::new(format!("  UDP  {}:{}", s.local_addr, s.local_port))),
    );
    let listen_list = List::new(listen_items)
        .block(
            Block::default()
                .title(format!(" Listening ({}) ", listen_count))
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::Green));
    f.render_widget(listen_list, listen_chunk);

    // Connections table
    let conn_rows: Vec<Row> = net
        .connections
        .iter()
        .take(max_connections())
        .map(|c| {
            let state_color = match c.state.as_str() {
                "ESTABLISHED" => Color::Green,
                "CLOSE_WAIT" | "FIN_WAIT1" | "FIN_WAIT2" => Color::Yellow,
                "TIME_WAIT" => Color::DarkGray,
                _ => Color::White,
            };
            Row::new(vec![
                Cell::from(c.protocol.clone()),
                Cell::from(format!("{}:{}", c.local_addr, c.local_port)),
                Cell::from(format!("{}:{}", c.remote_addr, c.remote_port)),
                Cell::from(c.state.clone()).style(Style::default().fg(state_color)),
            ])
        })
        .collect();

    let conn_table = Table::new(
        conn_rows,
        [
            Constraint::Length(6),
            Constraint::Length(22),
            Constraint::Length(22),
            Constraint::Length(14),
        ],
    )
    .block(
        Block::default()
            .title(format!(" Connections ({}) ", net.connections.len()))
            .borders(Borders::ALL),
    )
    .header(
        Row::new(vec!["Proto", "Local", "Remote", "State"]).style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        ),
    );
    f.render_widget(conn_table, table_chunk);
}

// ─── Files tab ────────────────────────────────────────────────────────────────

pub(crate) fn draw_files(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let files = match &info.open_files {
        Some(f) => f,
        None => {
            let p = Paragraph::new("Run with -f / --files to collect open file descriptors.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let rows: Vec<Row> = files
        .iter()
        .take(max_open_files())
        .map(|f| {
            let type_color = match f.fd_type.as_str() {
                "socket" => Color::Cyan,
                "pipe" => Color::Yellow,
                "anon_inode" => Color::Magenta,
                "device" => Color::Red,
                _ => Color::White,
            };
            Row::new(vec![
                Cell::from(f.fd.to_string()),
                Cell::from(f.fd_type.clone()).style(Style::default().fg(type_color)),
                Cell::from(f.description.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Min(0),
        ],
    )
    .block(
        Block::default()
            .title(format!(" Open Files ({}) ", files.len()))
            .borders(Borders::ALL),
    )
    .header(
        Row::new(vec!["FD", "Type", "Path / Description"]).style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        ),
    );
    f.render_widget(table, area);
}

// ─── Env tab ─────────────────────────────────────────────────────────────────

pub(crate) fn draw_env(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let env_vars = match &info.env_vars {
        Some(e) => e,
        None => {
            let p = Paragraph::new("Run with -e / --env to collect environment variables.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let secret_count = env_vars.iter().filter(|v| v.redacted).count();
    let title = format!(
        " Environment ({} vars, {} redacted) ",
        env_vars.len(),
        secret_count
    );

    let rows: Vec<Row> = env_vars
        .iter()
        .map(|v| {
            let (key_style, val_style) = if v.redacted {
                (
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::Red),
                )
            } else {
                (
                    Style::default().fg(Color::Cyan),
                    Style::default().fg(Color::White),
                )
            };
            Row::new(vec![
                Cell::from(v.key.clone()).style(key_style),
                Cell::from(v.value.clone()).style(val_style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(35), Constraint::Percentage(65)],
    )
    .block(Block::default().title(title).borders(Borders::ALL))
    .header(
        Row::new(vec!["Key", "Value"]).style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        ),
    );
    f.render_widget(table, area);
}

// ─── GPU tab ──────────────────────────────────────────────────────────────────

pub(crate) fn draw_gpu(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let gpus = match &info.gpu {
        Some(g) if !g.is_empty() => g,
        _ => {
            let p = Paragraph::new(
                "No GPU data (NVIDIA/AMD on Linux; system-wide stats when available)",
            )
            .block(Block::default().title(" GPU ").borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let rows: Vec<Row> = gpus
        .iter()
        .map(|g| {
            let util = g
                .utilization_percent
                .map(|u| format!("{:.1}%", u))
                .unwrap_or_else(|| "-".to_string());
            let mem_used = g
                .memory_used_mb
                .map(|u| format!("{:.0} MB", u))
                .unwrap_or_else(|| "-".to_string());
            let mem_total = g
                .memory_total_mb
                .map(|t| format!("{:.0} MB", t))
                .unwrap_or_else(|| "-".to_string());
            let process_mb = g
                .process_used_mb
                .map(|u| format!("{:.0} MB", u))
                .unwrap_or_else(|| "-".to_string());
            Row::new(vec![
                Cell::from(g.index.to_string()),
                Cell::from(g.name.clone()),
                Cell::from(util),
                Cell::from(mem_used),
                Cell::from(mem_total),
                Cell::from(process_mb).style(Style::default().fg(Color::Cyan)),
                Cell::from(g.source.clone()).style(Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Min(12),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .block(
        Block::default()
            .title(format!(" GPU ({}) ", gpus.len()))
            .borders(Borders::ALL),
    )
    .header(
        Row::new(vec![
            "#",
            "Name",
            "Util",
            "Mem Used",
            "Mem Total",
            "Process",
            "Source",
        ])
        .style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        ),
    );
    f.render_widget(table, area);
}

// ─── Tree tab ─────────────────────────────────────────────────────────────────

pub(crate) fn draw_tree(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let tree = match &info.process_tree {
        Some(t) => t,
        None => {
            let p = Paragraph::new("Run with -t / --tree to collect the process tree.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, area);
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();
    render_tree_lines(tree, "", true, info.pid, &mut lines);

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Process Tree ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_tree_lines<'a>(
    node: &'a ProcessNode,
    prefix: &str,
    is_last: bool,
    target_pid: i32,
    out: &mut Vec<Line<'a>>,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let label = format!(
        "{}{}{} ({})  [{} MB]",
        prefix,
        connector,
        node.name,
        node.pid,
        node.rss_kb / 1024
    );
    let style = if node.pid == target_pid {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    out.push(Line::from(Span::styled(label, style)));

    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    for (i, child) in node.children.iter().enumerate() {
        render_tree_lines(
            child,
            &child_prefix,
            i == node.children.len() - 1,
            target_pid,
            out,
        );
    }
}

// ─── Footer ───────────────────────────────────────────────────────────────────

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let tick_str = if app.paused {
        " ⏸ PAUSED ".to_string()
    } else {
        format!(" tick #{} ", app.tick)
    };

    let spans = vec![
        Span::styled("[q] quit", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("[Tab/←→] switch tab", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("[1–7] jump to tab", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("[Space] pause", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("[?] help", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(tick_str, Style::default().fg(Color::DarkGray)),
    ];

    let p = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn cpu_color(pct: f64) -> Color {
    if pct > 80.0 {
        Color::Red
    } else if pct > 50.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn mem_color(pct: f64) -> Color {
    if pct > 80.0 {
        Color::Red
    } else if pct > 50.0 {
        Color::Yellow
    } else {
        Color::Cyan
    }
}

/// RAM usage as % of total. Linux: /proc/meminfo; other platforms: sysinfo (so gauge is meaningful on macOS/Windows).
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

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(vertical[1]);
    horizontal[1]
}

fn draw_help_overlay(f: &mut Frame, _app: &App, area: Rect) {
    let block_area = centered_rect(70, 80, area);

    let text = vec![
        Line::from(Span::styled(
            "Keyboard shortcuts",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  q / Ctrl-C", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  quit"),
        ]),
        Line::from(vec![
            Span::styled("  Tab / ← → / h / l", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  switch tab"),
        ]),
        Line::from(vec![
            Span::styled("  1–7", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  jump directly to a tab"),
        ]),
        Line::from(vec![
            Span::styled("  Space", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  pause / resume updates"),
        ]),
        Line::from(vec![
            Span::styled("  ?", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  toggle this help overlay"),
        ]),
    ];

    let help = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(help, block_area);
}

fn max_connections() -> usize {
    std::env::var("PEEK_TUI_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0 && *n <= 1000)
        .unwrap_or(50)
}

fn max_open_files() -> usize {
    std::env::var("PEEK_TUI_MAX_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0 && *n <= 5000)
        .unwrap_or(200)
}
