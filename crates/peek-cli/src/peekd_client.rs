use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub const SOCKET_PATH: &str = "/run/peekd/peekd.sock";

#[derive(Debug, Serialize)]
struct Request<'a> {
    action: &'a str,
    pid: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct Response {
    pub ok: bool,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HistorySample {
    pub ts: String,
    pub rss_kb: u64,
    pub vm_size_kb: u64,
    pub threads: i32,
    pub cpu_percent: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AlertRuleRow {
    pub id: String,
    pub pid: i32,
    pub metric: String,
    pub threshold: f64,
    pub cooldown_secs: u64,
}

fn send_request(action: &str, pid: Option<i32>) -> Result<Response> {
    let req = Request { action, pid };
    send_request_value(serde_json::to_value(req)?)
}

/// Effective peekd socket path: env PEEK_PEEKD_SOCKET > config file > default.
pub fn effective_socket_path() -> String {
    std::env::var("PEEK_PEEKD_SOCKET")
        .ok()
        .or_else(|| {
            peek_core::config::load_config()
                .and_then(|c| c.peekd.socket_path)
        })
        .unwrap_or_else(|| SOCKET_PATH.to_string())
}

fn send_request_value(req: serde_json::Value) -> Result<Response> {
    let socket_path = effective_socket_path();
    let stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("cannot connect to peekd at {}", socket_path))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let mut json = serde_json::to_string(&req)?;
    json.push('\n');

    let mut writer = stream.try_clone()?;
    writer.write_all(json.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line)?;
    let resp: Response = serde_json::from_str(resp_line.trim())?;
    Ok(resp)
}

pub fn fetch_history(pid: i32) -> Result<Vec<HistorySample>> {
    let resp = send_request("history", Some(pid))?;
    if !resp.ok {
        anyhow::bail!(
            "{}",
            resp.message
                .unwrap_or_else(|| "unknown peekd error".to_string())
        );
    }
    let data = resp.data.context("no data in response")?;
    let samples: Vec<HistorySample> = serde_json::from_value(data)?;
    Ok(samples)
}

pub fn ping() -> bool {
    send_request("ping", None).map(|r| r.ok).unwrap_or(false)
}

pub fn register_watch(pid: i32) -> Result<()> {
    let resp = send_request("watch", Some(pid))?;
    if !resp.ok {
        anyhow::bail!("{}", resp.message.unwrap_or_default());
    }
    Ok(())
}

/// Add an alert rule. `comparison` must be "greater_than" or "less_than".
/// `notify` is "log", "stderr", or "script:<command>".
pub fn alert_add(
    pid: i32,
    metric: &str,
    comparison: &str,
    threshold: f64,
    notify: &str,
    cooldown_secs: Option<u64>,
) -> Result<String> {
    let comp_normalized = comparison.to_lowercase();
    let comparison = match comp_normalized.as_str() {
        "gt" | "greater_than" | ">" => "greater_than",
        "lt" | "less_than" | "<" => "less_than",
        other => other,
    };
    let notify_value = if notify.starts_with("script:") {
        serde_json::json!({ "type": "script", "command": notify.strip_prefix("script:").unwrap_or("") })
    } else if notify == "stderr" {
        serde_json::json!({ "type": "stderr" })
    } else {
        serde_json::json!({ "type": "log" })
    };
    let mut req = serde_json::json!({
        "action": "alert_add",
        "pid": pid,
        "metric": metric,
        "comparison": comparison,
        "threshold": threshold,
        "notify": notify_value
    });
    if let Some(c) = cooldown_secs {
        req["cooldown_secs"] = serde_json::json!(c);
    }
    let resp = send_request_value(req)?;
    if !resp.ok {
        anyhow::bail!(
            "{}",
            resp.message
                .unwrap_or_else(|| "unknown peekd error".to_string())
        );
    }
    let rule_id = resp
        .data
        .and_then(|d| d.get("rule_id").and_then(|v| v.as_str().map(String::from)))
        .context("no rule_id in response")?;
    Ok(rule_id)
}

pub fn alert_list() -> Result<Vec<AlertRuleRow>> {
    let resp = send_request_value(serde_json::json!({ "action": "alert_list" }))?;
    if !resp.ok {
        anyhow::bail!(
            "{}",
            resp.message
                .unwrap_or_else(|| "unknown peekd error".to_string())
        );
    }
    let data = resp.data.context("no data in response")?;
    let rules: Vec<AlertRuleRow> = data
        .get("rules")
        .and_then(|r| serde_json::from_value(r.clone()).ok())
        .unwrap_or_default();
    Ok(rules)
}

pub fn alert_remove(rule_id: &str) -> Result<bool> {
    let resp = send_request_value(serde_json::json!({
        "action": "alert_remove",
        "rule_id": rule_id,
    }))?;
    if !resp.ok {
        anyhow::bail!(
            "{}",
            resp.message
                .unwrap_or_else(|| "unknown peekd error".to_string())
        );
    }
    let removed = resp
        .data
        .and_then(|d| d.get("removed").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    Ok(removed)
}
