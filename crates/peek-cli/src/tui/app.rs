use std::time::Duration;

use peek_core::ringbuf::{detect_fd_leak, ResourceSample, RingBuf};
use peek_core::{CollectOptions, ProcessInfo};

const HISTORY_LEN: usize = 120; // 2 min at 1s default

fn history_len() -> usize {
    std::env::var("PEEK_HISTORY_LEN")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0 && *n <= 3600)
        .unwrap_or(HISTORY_LEN)
}

pub const TABS: &[&str] = &[
    "Overview", "Kernel", "Network", "Files", "Env", "Tree", "GPU",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdLeakStatus {
    Ok,
    Warning { start: usize, end: usize, n: usize },
}

pub struct App {
    pub pid: i32,
    pub opts: CollectOptions,
    pub interval: Duration,

    pub info: Option<ProcessInfo>,
    pub error: Option<String>,

    // Sparkline data (u64 for ratatui)
    pub cpu_history: RingBuf<ResourceSample>,
    pub cpu_spark: Vec<u64>,
    pub rss_spark: Vec<u64>,
    pub fd_spark: Vec<u64>,

    pub active_tab: usize,
    pub should_quit: bool,
    pub tick: u64,
    pub fd_leak: FdLeakStatus,
    pub paused: bool,
    pub show_help: bool,
}

impl App {
    pub fn new(pid: i32, opts: CollectOptions, interval: Duration) -> Self {
        Self {
            pid,
            opts,
            interval,
            info: None,
            error: None,
            cpu_history: RingBuf::new(history_len()),
            cpu_spark: Vec::new(),
            rss_spark: Vec::new(),
            fd_spark: Vec::new(),
            active_tab: 0,
            should_quit: false,
            tick: 0,
            fd_leak: FdLeakStatus::Ok,
            paused: false,
            show_help: false,
        }
    }

    pub fn refresh(&mut self) {
        if self.paused {
            return;
        }
        self.tick += 1;
        match peek_core::collect_extended(self.pid, &self.opts) {
            Ok(info) => {
                let sample = ResourceSample {
                    cpu_pct_x10: info.cpu_percent.unwrap_or(0.0) as u64 * 10,
                    rss_kb: info.rss_kb,
                    fd_count: info.fd_count.unwrap_or(0) as u64,
                    thread_count: info.threads as u64,
                };
                self.cpu_history.push(sample);

                // Precompute sparkline data once per refresh instead of on every draw
                self.cpu_spark = self
                    .cpu_history
                    .iter()
                    .map(|s| s.cpu_pct_x10.min(1000))
                    .collect();
                self.rss_spark = self.cpu_history.iter().map(|s| s.rss_kb / 1024).collect();
                self.fd_spark = self.cpu_history.iter().map(|s| s.fd_count).collect();

                // FD leak detection (check every 10 ticks)
                if self.tick.is_multiple_of(10) {
                    self.fd_leak = match detect_fd_leak(&self.cpu_history, 10) {
                        Some((start, end, n)) => FdLeakStatus::Warning { start, end, n },
                        None => FdLeakStatus::Ok,
                    };
                }

                self.info = Some(info);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % TABS.len();
    }

    pub fn prev_tab(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = TABS.len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }

    /// Sparkline data for CPU (0–1000, representing 0–100.0%)
    pub fn cpu_sparkline(&self) -> &[u64] {
        &self.cpu_spark
    }

    /// Sparkline data for RSS in MB (normalised to 0–max_rss_mb)
    pub fn rss_sparkline(&self) -> &[u64] {
        &self.rss_spark
    }

    /// Sparkline data for FD counts
    pub fn fd_sparkline(&self) -> &[u64] {
        &self.fd_spark
    }
}
