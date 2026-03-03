//! Core library for peek: process snapshot types, collection orchestration, and extended data.
//!
//! Provides `ProcessInfo`, `CollectOptions`, `collect()`, and `collect_extended()`; delegates to
//! proc-reader, kernel-explainer, resource-sampler, network-inspector, and signal-engine.

pub mod config;
pub mod proc;
pub mod ringbuf;

use serde::{Deserialize, Serialize};

// ─── Core process snapshot ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessInfo {
    pub pid: i32,
    pub name: String,
    pub cmdline: String,
    /// Resolved executable path from `/proc/<pid>/exe`, when available.
    pub exe: Option<String>,
    pub state: String,
    pub ppid: i32,
    pub uid: u32,
    pub gid: u32,
    pub started_at: Option<chrono::DateTime<chrono::Local>>,
    pub threads: i32,
    pub vm_size_kb: u64,
    pub rss_kb: u64,
    /// Proportional set size (KB), from smaps_rollup. Linux, extended only.
    pub pss_kb: Option<u64>,
    /// Swap used (KB), from status VmSwap. Linux, extended only.
    pub swap_kb: Option<u64>,
    // Extended resource fields
    pub cpu_percent: Option<f64>,
    pub io_read_bytes: Option<u64>,
    pub io_write_bytes: Option<u64>,
    pub fd_count: Option<usize>,
    // Optional rich sections
    pub kernel: Option<KernelInfo>,
    pub network: Option<NetworkInfo>,
    pub open_files: Option<Vec<OpenFile>>,
    pub env_vars: Option<Vec<EnvVar>>,
    pub process_tree: Option<ProcessNode>,
    pub gpu: Option<Vec<GpuInfo>>,
}

// ─── Kernel context ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KernelInfo {
    pub sched_policy: String,
    pub nice: i32,
    pub priority: i32,
    pub oom_score: i32,
    pub oom_score_adj: i32,
    pub cgroup: String,
    pub namespaces: Vec<NamespaceEntry>,
    pub cap_permitted: String,
    pub cap_effective: String,
    pub seccomp: u32,
    pub voluntary_ctxt_switches: Option<u64>,
    pub nonvoluntary_ctxt_switches: Option<u64>,
    /// Optional LSM security label (e.g. AppArmor/SELinux).
    pub security_label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NamespaceEntry {
    pub ns_type: String,
    pub inode: String,
}

// ─── Network ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkInfo {
    pub listening_tcp: Vec<SocketEntry>,
    pub listening_udp: Vec<SocketEntry>,
    pub connections: Vec<ConnectionEntry>,
    /// Unix socket paths for this process (from /proc/net/unix + fd inodes).
    pub unix_sockets: Option<Vec<UnixSocketEntry>>,
    /// RX bytes/sec in process network namespace (from /proc/<pid>/net/dev delta).
    /// Sampling window is controlled by `PEEK_NET_SAMPLE_MS` (default 1000ms; 0 disables sampling).
    pub traffic_rx_bytes_per_sec: Option<u64>,
    /// TX bytes/sec in process network namespace.
    pub traffic_tx_bytes_per_sec: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnixSocketEntry {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SocketEntry {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionEntry {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
}

// ─── Files ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenFile {
    pub fd: u32,
    pub fd_type: String,
    pub description: String,
}

// ─── Environment ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
    pub redacted: bool,
}

// ─── Process tree ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessNode {
    pub pid: i32,
    pub name: String,
    pub uid: u32,
    pub rss_kb: u64,
    pub children: Vec<ProcessNode>,
}

// ─── GPU ─────────────────────────────────────────────────────────────────────

pub use resource_sampler::gpu::GpuInfo;

// ─── Signal impact pre-flight ─────────────────────────────────────────────────
pub use signal_engine::impact::SignalImpact;

// ─── FD leak detection ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FdLeakWarning {
    /// FD count at the start of the observation window.
    pub start_count: usize,
    /// FD count at the end.
    pub end_count: usize,
    /// How many consecutive samples showed an increase.
    pub consecutive_increases: usize,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum PeekError {
    #[error("process {0} not found")]
    NotFound(i32),

    #[error("failed to read /proc for pid {pid}: {source}")]
    ProcIo {
        pid: i32,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse /proc for pid {pid}: {msg}")]
    ProcParse { pid: i32, msg: String },
}

impl From<proc_reader::ProcReaderError> for PeekError {
    fn from(e: proc_reader::ProcReaderError) -> Self {
        let pid = e.pid().unwrap_or(-1);
        match e {
            proc_reader::ProcReaderError::NotFound(pid) => PeekError::NotFound(pid),
            proc_reader::ProcReaderError::Io { source, .. } => PeekError::ProcIo { pid, source },
            proc_reader::ProcReaderError::Parse { msg, .. } => PeekError::ProcParse { pid, msg },
        }
    }
}

pub type Result<T> = std::result::Result<T, PeekError>;

// ─── Collect options ─────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct CollectOptions {
    pub resources: bool,
    pub kernel: bool,
    pub network: bool,
    pub files: bool,
    pub env: bool,
    pub tree: bool,
    pub gpu: bool,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Fast baseline snapshot (no CPU sampling, no extended sections).
pub fn collect(pid: i32) -> Result<ProcessInfo> {
    proc::collect_process(pid, false)
}

/// Full snapshot gated by `opts`. On Linux includes kernel, network, files, env, tree, GPU; elsewhere baseline only.
pub fn collect_extended(pid: i32, opts: &CollectOptions) -> Result<ProcessInfo> {
    let mut info = proc::collect_process(pid, opts.resources)?;

    #[cfg(target_os = "linux")]
    {
        if opts.resources {
            info.io_read_bytes = proc::resources::read_io(pid).map(|io| io.0).ok();
            info.io_write_bytes = proc::resources::read_io(pid).map(|io| io.1).ok();
            info.fd_count = proc::files::count_fds(pid).ok();
            if let Some((_rss, pss, swap)) = resource_sampler::memory::sample_memory(pid) {
                info.pss_kb = Some(pss);
                info.swap_kb = Some(swap);
            }
        }
        if opts.kernel {
            info.kernel = proc::kernel::collect_kernel(pid).ok();
        }
        if opts.network {
            info.network = proc::network::collect_network(pid).ok();
        }
        if opts.files {
            info.open_files = proc::files::collect_files(pid).ok();
        }
        if opts.env {
            info.env_vars = proc::env::collect_env(pid).ok();
        }
        if opts.tree {
            info.process_tree = proc::tree::build_tree(pid).ok();
        }
        if opts.gpu {
            info.gpu = Some(proc::gpu::collect_gpu(pid));
        }
    }

    Ok(info)
}

/// Pre-flight signal impact analysis. Linux only.
pub fn signal_impact(pid: i32) -> anyhow::Result<SignalImpact> {
    #[cfg(target_os = "linux")]
    {
        signal_engine::impact::analyze_impact(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("Signal impact analysis is only available on Linux")
    }
}

/// Optional human-readable description for well-known process names.
pub fn binary_description(name: &str) -> Option<String> {
    kernel_explainer::well_known::binary_description(name).map(|s| s.to_string())
}

/// Human-readable OOM kill likelihood band (low / moderate / high / critical).
pub fn oom_description(score: i32) -> &'static str {
    kernel_explainer::oom::oom_description(score)
}

/// Soft "Max open files" limit from `/proc/<pid>/limits`, if available.
#[cfg(target_os = "linux")]
pub fn fd_soft_limit(pid: i32) -> Option<u64> {
    use proc_reader::limits::read_limits;

    let limits = read_limits(pid).ok()?;
    limits.max_open_files_soft
}

#[cfg(not(target_os = "linux"))]
pub fn fd_soft_limit(_pid: i32) -> Option<u64> {
    None
}

/// Current syscall name and description from `/proc/<pid>/syscall` (x86_64).
/// Returns `None` if unreadable or syscall number unknown.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn current_syscall(pid: i32) -> Option<(String, String)> {
    let (num, _) = proc_reader::current::read_syscall(pid)?;
    let name = kernel_explainer::syscalls::syscall_name_x86_64(num)?;
    let desc = kernel_explainer::syscalls::syscall_description(name);
    Some((name.to_string(), desc.to_string()))
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn current_syscall(_pid: i32) -> Option<(String, String)> {
    None
}

/// Best-effort reverse DNS for an address (e.g. "192.168.1.1:443"). Time-bounded; for CLI/TUI only.
pub fn resolve_remote(addr: &str) -> Option<String> {
    network_inspector::resolver::resolve(addr)
}
