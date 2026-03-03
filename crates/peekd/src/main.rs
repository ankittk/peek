// peekd daemon: main entry, wires watcher + ring_store + alert + socket. Plan: main, watcher, ring_store, alert, socket.

mod alert;
mod ring_store;
mod socket;
mod watcher;

#[cfg(unix)]
pub const SOCKET_PATH: &str = "/run/peekd/peekd.sock";

#[cfg(unix)]
pub const PID_PATH: &str = "/run/peekd/peekd.pid";

#[cfg(unix)]
pub fn max_watched_pids() -> usize {
    std::env::var("PEEKD_MAX_WATCHED_PIDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0 && *n <= 10_000)
        .unwrap_or(256)
}

#[cfg(not(unix))]
fn main() {
    eprintln!("peekd is only supported on Linux/Unix.");
    std::process::exit(1);
}

#[cfg(unix)]
fn main() {
    if let Err(e) = run() {
        eprintln!("peekd: {:#}", e);
        std::process::exit(1);
    }
}

#[cfg(unix)]
fn run() -> anyhow::Result<()> {
    tokio::runtime::Runtime::new()?.block_on(daemon_main())
}

#[cfg(unix)]
struct PidFileGuard(std::path::PathBuf);

#[cfg(unix)]
impl Drop for PidFileGuard {
    fn drop(&mut self) {
        use std::io::ErrorKind;
        if let Err(e) = std::fs::remove_file(&self.0) {
            if e.kind() != ErrorKind::NotFound {
                tracing::debug!(
                    "peekd: failed to remove pid file {}: {}",
                    self.0.display(),
                    e
                );
            }
        }
    }
}

#[cfg(unix)]
fn acquire_pid_file(path: &std::path::Path) -> anyhow::Result<PidFileGuard> {
    use anyhow::Context as _;
    use std::fs::OpenOptions;
    use std::io::{ErrorKind, Read, Write};

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut f) => {
            write!(f, "{}", std::process::id()).context("write pid to pidfile")?;
            Ok(PidFileGuard(path.to_path_buf()))
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            // Try to detect a still-running daemon; otherwise treat as stale and overwrite.
            let mut existing = String::new();
            if let Ok(mut f) = OpenOptions::new().read(true).open(path) {
                let _ = f.read_to_string(&mut existing);
            }
            if let Ok(pid) = existing.trim().parse::<u32>() {
                let proc_path = std::path::Path::new("/proc").join(pid.to_string());
                if proc_path.exists() {
                    anyhow::bail!("peekd already running with pid {}", pid);
                }
            }

            let mut f = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)
                .context("open stale pidfile for overwrite")?;
            write!(f, "{}", std::process::id()).context("rewrite pid to pidfile")?;
            Ok(PidFileGuard(path.to_path_buf()))
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(unix)]
async fn daemon_main() -> anyhow::Result<()> {
    use alert::{load_config_into, AlertEngine};
    use std::sync::{Arc, Mutex};
    use tokio::signal::unix::{signal, SignalKind};
    use tokio::sync::watch;
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let socket_path: String = std::env::var("PEEK_PEEKD_SOCKET")
        .ok()
        .or_else(|| {
            peek_core::config::load_config().and_then(|c| c.peekd.socket_path)
        })
        .unwrap_or_else(|| SOCKET_PATH.to_string());

    if let Some(dir) = std::path::Path::new(&socket_path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _pid_guard = acquire_pid_file(std::path::Path::new(PID_PATH))?;

    tracing::info!("peekd starting (socket: {}, pidfile: {})", socket_path, PID_PATH);

    let _ = std::fs::remove_file(&socket_path);

    let history = ring_store::new_history();
    let mut watched_vec: Vec<i32> = Vec::new();
    let mut engine = AlertEngine::new();
    if let Err(e) = load_config_into(&mut engine, &mut watched_vec) {
        tracing::warn!("failed to load alerts config: {}", e);
    }
    let max_watched = max_watched_pids();
    if watched_vec.len() > max_watched {
        watched_vec.truncate(max_watched);
        tracing::warn!(
            "initial watched PID list truncated to {} entries (limit {})",
            watched_vec.len(),
            max_watched
        );
    }
    let watched: watcher::WatchedPids = Arc::new(Mutex::new(watched_vec));
    let alerts: watcher::AlertEng = Arc::new(Mutex::new(engine));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    {
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let shutdown_tx = shutdown_tx.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("peekd received SIGTERM, initiating shutdown");
                }
                _ = sigint.recv() => {
                    tracing::info!("peekd received SIGINT, initiating shutdown");
                }
            }
            let _ = shutdown_tx.send(true);
        });
    }

    watcher::run(history.clone(), watched.clone(), alerts.clone(), shutdown_rx.clone());

    let res = socket::run_listener(&socket_path, history, watched, alerts, shutdown_rx).await;

    // Dropping the sender signals any remaining tasks to exit if they haven't already.
    drop(shutdown_tx);

    res
}
