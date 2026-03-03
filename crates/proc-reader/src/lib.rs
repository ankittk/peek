//! Low-level parsing of `/proc/<PID>/*` and sysfs.
//!
//! Reads fd, environ, cgroup, limits, and related files into raw structs.
//! Does not depend on peek-core; used by peek-core and peekd.
//!
//! All public APIs return typed [ProcReaderError] so consumers can match on
//! [ProcReaderError::NotFound], [ProcReaderError::Io], or [ProcReaderError::Parse].

pub mod cgroup;
pub mod current;
pub mod environ;
pub mod error;
pub mod fd;
pub mod limits;
pub mod security;

pub use error::{ProcReaderError, Result};
