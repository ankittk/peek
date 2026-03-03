// Sampling loop: poll watched PIDs every second, store samples, evaluate alerts. Plan: watcher.

#[cfg(unix)]
pub type WatchedPids = std::sync::Arc<std::sync::Mutex<Vec<i32>>>;

#[cfg(unix)]
pub type AlertEng = std::sync::Arc<std::sync::Mutex<crate::alert::AlertEngine>>;

#[cfg(unix)]
fn sample_interval() -> tokio::time::Duration {
    use tokio::time::Duration;

    let secs = std::env::var("PEEKD_SAMPLE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &u64| *n > 0 && *n <= 3600)
        .unwrap_or(1);
    Duration::from_secs(secs)
}

#[cfg(unix)]
pub fn run(
    history: crate::ring_store::History,
    watched: WatchedPids,
    alerts: AlertEng,
    shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use chrono::Local;
    use peek_core::collect;
    use tokio::time::interval;

    tokio::spawn(async move {
        let mut shutdown = shutdown;
        let mut ticker = interval(sample_interval());
        loop {
            tokio::select! {
                _ = ticker.tick() => {
            let pids: Vec<i32> = lock_arc_unpoison(&watched, "watched PIDs").clone();
            for pid in pids {
                match collect(pid) {
                    Ok(info) => {
                        let sample = crate::ring_store::Sample {
                            ts: Local::now(),
                            rss_kb: info.rss_kb,
                            vm_size_kb: info.vm_size_kb,
                            threads: info.threads,
                            cpu_percent: info.cpu_percent,
                            fd_count: info.fd_count,
                        };
                        crate::ring_store::push_sample(&history, pid, sample);
                        let fired = lock_arc_unpoison(&alerts, "alerts").evaluate(&info);
                        for event in fired {
                            tracing::warn!(
                                "alert fired: rule={} pid={} {}={:.2} threshold={:.2}",
                                event.rule_id,
                                event.pid,
                                event.metric,
                                event.value,
                                event.threshold
                            );
                        }
                    }
                    Err(_) => {
                        tracing::info!("pid {} gone, removing", pid);
                        lock_arc_unpoison(&watched, "watched PIDs").retain(|&p| p != pid);
                        crate::ring_store::remove_pid(&history, pid);
                        lock_arc_unpoison(&alerts, "alerts").remove_rules_for_pid(pid);
                    }
                }
            }
                }
                changed = shutdown.changed() => {
                    match changed {
                        Ok(_) => {
                            if *shutdown.borrow() {
                                tracing::info!("shutdown requested, stopping watcher loop");
                                break;
                            }
                        }
                        Err(_) => {
                            tracing::info!("shutdown channel closed, stopping watcher loop");
                            break;
                        }
                    }
                }
            }
        }
    });
}

#[cfg(unix)]
fn lock_arc_unpoison<'a, T>(
    arc: &'a std::sync::Arc<std::sync::Mutex<T>>,
    name: &str,
) -> std::sync::MutexGuard<'a, T> {
    match arc.lock() {
        Ok(guard) => guard,
        Err(e) => {
            tracing::error!("{} mutex poisoned; continuing with inner value", name);
            e.into_inner()
        }
    }
}
