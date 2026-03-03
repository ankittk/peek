//! General config file support: `~/.config/peek/config.toml` (or XDG_CONFIG_HOME/peek/config.toml).
//!
//! Used by peek-cli for default flags and peekd socket path, and by peekd for socket path.
//! Env vars (e.g. PEEK_PEEKD_SOCKET) override config file values.
//!
//! Example `config.toml`:
//!
//! ```toml
//! [defaults]
//! no-color = false
//! resolve = true
//!
//! [peekd]
//! socket-path = "/run/peekd/peekd.sock"
//!
//! [export]
//! default-format = "md"
//! ```

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct PeekConfig {
    pub defaults: DefaultsSection,
    pub peekd: PeekdSection,
    pub export: ExportSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct DefaultsSection {
    /// Default for --no-color (disable colored output).
    pub no_color: bool,
    /// Default for --resolve (resolve remote addresses).
    pub resolve: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct PeekdSection {
    /// Unix socket path for peekd. Overridden by PEEK_PEEKD_SOCKET.
    pub socket_path: Option<String>,
    /// Directory for history JSONL files. Overridden by XDG_STATE_HOME / ~/.local/state/peekd.
    pub history_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct ExportSection {
    /// Default export/output format: json | json_snapshot | md | html | pdf.
    pub default_format: Option<String>,
}

/// Returns the path to the config file (XDG_CONFIG_HOME/peek/config.toml or ~/.config/peek/config.toml).
/// Does not check if the file exists.
pub fn config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("peek").join("config.toml");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("peek")
            .join("config.toml");
    }
    PathBuf::from(".config").join("peek").join("config.toml")
}

/// Load config from the standard location. Returns `None` if the file is missing or invalid.
pub fn load_config() -> Option<PeekConfig> {
    let path = config_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&raw).ok()
}
