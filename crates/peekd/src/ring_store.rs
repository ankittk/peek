// Ring-buffer history storage per PID, with optional on-disk persistence. Plan: ring_store.

#[cfg(unix)]
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Sample {
    pub ts: chrono::DateTime<chrono::Local>,
    pub rss_kb: u64,
    pub vm_size_kb: u64,
    pub threads: i32,
    pub cpu_percent: Option<f64>,
    pub fd_count: Option<usize>,
}

#[cfg(unix)]
const DEFAULT_RING_SIZE: usize = 300; // 5 min at 1 s

#[cfg(unix)]
fn ring_size() -> usize {
    std::env::var("PEEKD_RING_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0 && *n <= 3600)
        .unwrap_or(DEFAULT_RING_SIZE)
}

#[cfg(unix)]
pub type History = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i32, Vec<Sample>>>>;

#[cfg(unix)]
pub fn new_history() -> History {
    std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(unix)]
fn lock_history(
    history: &History,
) -> std::sync::MutexGuard<'_, std::collections::HashMap<i32, Vec<Sample>>> {
    match history.lock() {
        Ok(guard) => guard,
        Err(e) => {
            tracing::error!("history mutex poisoned; continuing with inner value");
            e.into_inner()
        }
    }
}

#[cfg(unix)]
pub fn push_sample(history: &History, pid: i32, sample: Sample) {
    let mut h = lock_history(history);
    let ring = h.entry(pid).or_default();
    ring.push(sample);
    if ring.len() > ring_size() {
        ring.remove(0);
    }

    // Fire-and-forget append to on-disk history (JSONL per PID). Errors are logged only.
    if let Err(e) = append_sample_to_disk(pid, ring.last().unwrap()) {
        tracing::debug!("failed to append history for pid {}: {}", pid, e);
    }
}

#[cfg(unix)]
pub fn remove_pid(history: &History, pid: i32) {
    lock_history(history).remove(&pid);
}

// ─── On-disk persistence ──────────────────────────────────────────────────────

#[cfg(unix)]
fn history_root() -> std::path::PathBuf {
    // Prefer XDG_STATE_HOME/peekd, then ~/.local/state/peekd, then /var/lib/peekd.
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        return std::path::Path::new(&xdg).join("peekd");
    }
    if let Ok(home) = std::env::var("HOME") {
        return std::path::Path::new(&home)
            .join(".local")
            .join("state")
            .join("peekd");
    }
    std::path::PathBuf::from("/var/lib/peekd")
}

#[cfg(unix)]
fn history_path(pid: i32) -> std::path::PathBuf {
    history_root().join(format!("{}.jsonl", pid))
}

#[cfg(unix)]
fn append_sample_to_disk(pid: i32, sample: &Sample) -> anyhow::Result<()> {
    use std::io::Write;
    let root = history_root();
    std::fs::create_dir_all(&root)?;
    let path = history_path(pid);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(sample)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Lazily load history for `pid` from disk into the in-memory ring, if present.
#[cfg(unix)]
pub fn load_from_disk(history: &History, pid: i32) {
    let path = history_path(pid);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut h = lock_history(history);
    let ring = h.entry(pid).or_default();
    ring.clear();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(sample) = serde_json::from_str::<Sample>(line) {
            ring.push(sample);
            if ring.len() > ring_size() {
                ring.remove(0);
            }
        }
    }
}
