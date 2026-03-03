use crate::{EnvVar, Result};
use proc_reader::environ::{read_environ, EnvironEntry};

// Key-based patterns for secret-ish variables. These are deliberately broad and
// err on the side of redacting more rather than less.
const SECRET_KEY_PATTERNS: &[&str] = &[
    "PASSWORD",
    "PASSWD",
    "PWD",
    "SECRET",
    "TOKEN",
    "API_KEY",
    "APIKEY",
    "AUTH",
    "CREDENTIAL",
    "PRIVATE_KEY",
    "ACCESS_KEY",
    "AWS_ACCESS_KEY",
    "AWS_SECRET",
    "DATABASE_URL",
    "DB_URL",
    "REDIS_URL",
    "MONGO_URL",
    "DSN",
    "SLACK_WEBHOOK",
    "WEBHOOK_URL",
];

pub fn collect_env(pid: i32) -> Result<Vec<EnvVar>> {
    let raw: Vec<EnvironEntry> = read_environ(pid)?;
    let mut vars = Vec::with_capacity(raw.len());

    for entry in raw {
        let redacted = is_secret(&entry.key, &entry.value);
        vars.push(EnvVar {
            key: entry.key,
            value: if redacted {
                "***".to_string()
            } else {
                entry.value
            },
            redacted,
        });
    }

    vars.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(vars)
}

fn is_secret(key: &str, value: &str) -> bool {
    let upper_key = key.to_uppercase();
    if SECRET_KEY_PATTERNS.iter().any(|p| upper_key.contains(p)) {
        return true;
    }

    // Value-based heuristics (best-effort):
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Looks like a private key block.
    if trimmed.starts_with("-----BEGIN ") {
        return true;
    }

    // Looks like an AWS access key or secret.
    if trimmed.starts_with("AKIA") && trimmed.len() >= 16 {
        return true;
    }

    // Very long tokens are likely secrets (JWTs, API tokens, etc.).
    if trimmed.len() >= 40 {
        return true;
    }

    false
}
