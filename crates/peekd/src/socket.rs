// Unix socket accept loop and request dispatch. Plan: socket.

#[cfg(unix)]
use crate::alert;
#[cfg(unix)]
use crate::ring_store;
#[cfg(unix)]
use crate::watcher::{AlertEng, WatchedPids};

#[cfg(unix)]
fn max_connections() -> usize {
    std::env::var("PEEKD_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0 && *n <= 1000)
        .unwrap_or(20)
}

#[cfg(unix)]
fn min_request_interval_ms() -> u64 {
    std::env::var("PEEKD_MIN_REQUEST_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &u64| *n <= 60_000)
        .unwrap_or(50)
}

#[cfg(unix)]
pub async fn run_listener(
    path: &str,
    history: ring_store::History,
    watched: WatchedPids,
    alerts: AlertEng,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use tokio::net::UnixListener;
    use tokio::sync::Semaphore;

    let listener = UnixListener::bind(path)?;
    let connection_limit = Arc::new(Semaphore::new(max_connections()));
    // Default to owner/group access only (0o660). Use your systemd unit or
    // filesystem ACLs to relax this if you explicitly want broader access.
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o660);
        let _ = std::fs::set_permissions(path, perms);
    }
    tracing::info!("peekd ready");

    loop {
        tokio::select! {
            res = listener.accept() => {
                match res {
                    Ok((stream, _)) => {
                        let permit = match connection_limit.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                drop(stream);
                                tracing::warn!("max connections ({}), dropping new connection", max_connections());
                                continue;
                            }
                        };
                        let history = std::sync::Arc::clone(&history);
                        let watched = std::sync::Arc::clone(&watched);
                        let alerts = std::sync::Arc::clone(&alerts);
                        let min_interval_ms = min_request_interval_ms();
                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Err(e) = handle_client(stream, history, watched, alerts, min_interval_ms).await {
                                tracing::warn!("client error: {}", e);
                            }
                        });
                    }
                    Err(e) => tracing::error!("accept error: {}", e),
                }
            }
            changed = shutdown.changed() => {
                match changed {
                    Ok(_) => {
                        if *shutdown.borrow() {
                            tracing::info!("shutdown requested, stopping socket listener");
                            break;
                        }
                    }
                    Err(_) => {
                        tracing::info!("shutdown channel closed, stopping socket listener");
                        break;
                    }
                }
            }
        }
    }

    if let Err(e) = std::fs::remove_file(path) {
        use std::io::ErrorKind;
        if e.kind() != ErrorKind::NotFound {
            tracing::debug!("failed to remove socket {} on shutdown: {}", path, e);
        }
    }

    Ok(())
}

#[cfg(unix)]
async fn handle_client(
    stream: tokio::net::UnixStream,
    history: ring_store::History,
    watched: WatchedPids,
    alerts: AlertEng,
    min_request_interval_ms: u64,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::Instant;

    let (r, mut w) = tokio::io::split(stream);
    let mut reader = BufReader::new(r);
    let mut line = String::new();
    let min_interval = std::time::Duration::from_millis(min_request_interval_ms);
    let mut last_request = Instant::now();
    let mut first = true;

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        if !first && last_request.elapsed() < min_interval {
            tokio::time::sleep(min_interval.saturating_sub(last_request.elapsed())).await;
        }
        first = false;
        last_request = Instant::now();

        let response = match serde_json::from_str::<serde_json::Value>(line.trim()) {
            Err(e) => json_err(format!("invalid JSON: {}", e)),
            Ok(req) => dispatch(req, &history, &watched, &alerts),
        };

        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        w.write_all(out.as_bytes()).await?;
    }
    Ok(())
}

#[cfg(unix)]
fn dispatch(
    req: serde_json::Value,
    history: &ring_store::History,
    watched: &WatchedPids,
    alerts: &AlertEng,
) -> serde_json::Value {
    let action = req["action"].as_str().unwrap_or("").to_string();

    match action.as_str() {
        "watch" => {
            let pid = match req["pid"].as_i64() {
                Some(p) => p as i32,
                None => return json_err("'watch' requires pid"),
            };
            let mut w = lock_arc_unpoison(watched, "watched PIDs");
            if !w.contains(&pid) {
                let max = crate::max_watched_pids();
                if w.len() >= max {
                    return json_err(format!(
                        "too many watched PIDs (limit {}) — adjust PEEKD_MAX_WATCHED_PIDS if needed",
                        max
                    ));
                }
                w.push(pid);
            }
            json_ok(serde_json::json!({ "watching": pid }))
        }
        "unwatch" => {
            let pid = match req["pid"].as_i64() {
                Some(p) => p as i32,
                None => return json_err("'unwatch' requires pid"),
            };
            lock_arc_unpoison(watched, "watched PIDs").retain(|&p| p != pid);
            ring_store::remove_pid(history, pid);
            lock_arc_unpoison(alerts, "alerts").remove_rules_for_pid(pid);
            json_ok(serde_json::json!({ "unwatched": pid }))
        }
        "list" => {
            let w = lock_arc_unpoison(watched, "watched PIDs").clone();
            json_ok(serde_json::json!({ "watching": w }))
        }
        "history" => {
            let pid = match req["pid"].as_i64() {
                Some(p) => p as i32,
                None => return json_err("'history' requires pid"),
            };
            // Lazy-load from disk if not present in memory.
            {
                let h = lock_arc_unpoison(history, "history");
                if !h.contains_key(&pid) {
                    drop(h);
                    crate::ring_store::load_from_disk(history, pid);
                }
            }
            let h = lock_arc_unpoison(history, "history");
            match h.get(&pid) {
                Some(samples) => {
                    let j = serde_json::to_value(samples).unwrap_or(serde_json::Value::Null);
                    json_ok(j)
                }
                None => json_err(format!(
                    "no history for pid {} — add it with 'watch' first",
                    pid
                )),
            }
        }
        "alert_add" => {
            let add_req: alert::AlertAddRequest = match serde_json::from_value(req.clone()) {
                Ok(r) => r,
                Err(e) => return json_err(format!("invalid alert rule: {}", e)),
            };
            let id = format!("rule-{}", chrono::Local::now().timestamp_millis());
            let rule = add_req.into_rule(id.clone());
            lock_arc_unpoison(alerts, "alerts").add_rule(rule);
            let pid_to_watch = req["pid"].as_i64().unwrap_or(0) as i32;
            if pid_to_watch > 0 {
                let mut w = lock_arc_unpoison(watched, "watched PIDs");
                if !w.contains(&pid_to_watch) {
                    let max = crate::max_watched_pids();
                    if w.len() >= max {
                        return json_err(format!(
                            "too many watched PIDs (limit {}) — adjust PEEKD_MAX_WATCHED_PIDS if needed",
                            max
                        ));
                    }
                    w.push(pid_to_watch);
                }
            }
            json_ok(serde_json::json!({ "rule_id": id }))
        }
        "alert_list" => {
            let eng = lock_arc_unpoison(alerts, "alerts");
            let rules: Vec<serde_json::Value> = eng
                .rules
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "pid": r.pid,
                        "metric": r.metric.to_string(),
                        "threshold": r.threshold,
                        "cooldown_secs": r.cooldown_secs,
                    })
                })
                .collect();
            json_ok(serde_json::json!({ "rules": rules }))
        }
        "alert_remove" => {
            let rule_id = match req["rule_id"].as_str() {
                Some(s) => s.to_string(),
                None => return json_err("'alert_remove' requires rule_id"),
            };
            let removed = lock_arc_unpoison(alerts, "alerts").remove_rule(&rule_id);
            json_ok(serde_json::json!({ "removed": removed, "rule_id": rule_id }))
        }
        "ping" => json_ok(serde_json::json!({
            "pong": true,
            "version": env!("CARGO_PKG_VERSION"),
            "watching": watched
                .lock()
                .map(|w| w.len())
                .unwrap_or_else(|e| {
                    tracing::error!("watched PIDs mutex poisoned in ping; continuing: {}", e);
                    e.into_inner().len()
                }),
        })),
        other => json_err(format!("unknown action '{}'", other)),
    }
}

#[cfg(unix)]
fn json_ok(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "ok": true, "data": data })
}

#[cfg(unix)]
fn json_err(msg: impl Into<String>) -> serde_json::Value {
    serde_json::json!({ "ok": false, "message": msg.into() })
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
