//! Typed errors for proc-reader so consumers can match on failure modes.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcReaderError {
    #[error("process {0} not found")]
    NotFound(i32),

    #[error("I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parse error in {path}: {msg}")]
    Parse { path: PathBuf, msg: String },
}

pub type Result<T> = std::result::Result<T, ProcReaderError>;

impl ProcReaderError {
    /// PID involved in the error, if known (from NotFound or from path like /proc/123/...).
    pub fn pid(&self) -> Option<i32> {
        match self {
            ProcReaderError::NotFound(pid) => Some(*pid),
            ProcReaderError::Io { path, .. } => pid_from_proc_path(path),
            ProcReaderError::Parse { path, .. } => pid_from_proc_path(path),
        }
    }
}

/// Extract PID from a path like `/proc/123/limits` or `/proc/123/fd`. Returns None if not /proc/<pid>/...
pub(crate) fn pid_from_proc_path(path: &std::path::Path) -> Option<i32> {
    let mut components = path.components();
    let _root = components.next()?;
    let proc_name = components.next()?.as_os_str().to_str()?;
    if proc_name != "proc" {
        return None;
    }
    let pid_str = components.next()?.as_os_str().to_str()?;
    pid_str.parse().ok()
}

/// Map io::Error to ProcReaderError; use path and optional pid for context.
pub(crate) fn io_to_error(path: PathBuf, e: std::io::Error, pid: i32) -> ProcReaderError {
    if e.kind() == std::io::ErrorKind::NotFound {
        ProcReaderError::NotFound(pid)
    } else {
        ProcReaderError::Io { path, source: e }
    }
}
