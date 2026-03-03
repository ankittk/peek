# peek architecture

Layout matches `plan.md` §2 (Repository Structure).

## Crates and responsibilities

- **peek-cli**
  - CLI binary (`main.rs`, `args.rs`) and `ui/` (overview, resources, network, files, kernel, tree).
  - Depends on `peek-core` for data and `export-engine` for JSON/MD/HTML/PDF output.

- **peek-core**
  - Orchestrator: owns `ProcessInfo`, `CollectOptions`, `collect()`, `collect_extended()`, and `signal_impact()`.
  - Delegates to helper crates:
    - `proc-reader` for `/proc/<PID>/*` and sysfs parsing.
    - `kernel-explainer` for human-readable kernel state/scheduler/capabilities.
    - `resource-sampler` for CPU/disk/GPU sampling and ring buffer utilities.
    - `network-inspector` for per-process TCP/UDP socket views.
    - `signal-engine` for impact analysis and systemd unit detection.

- **proc-reader**
  - Parses `/proc/<PID>/status`, `stat`, `statm`, `fd/*`, `environ`, `cgroup`, and related files into raw structs.
  - Intentionally does not depend on `peek-core` types; it is low-level I/O only.

- **kernel-explainer**
  - Converts raw kernel values into explanations:
    - State chars → labels (Running, Zombie, Uninterruptible sleep, …).
    - Scheduler policy/priority/nice → human descriptions.
    - Capability bitmasks → lists of named capabilities.
    - Namespaces, OOM scores, and other kernel metadata (extensible).

- **resource-sampler**
  - Samples CPU usage from `/proc/<PID>/stat` + `/proc/stat`.
  - Reads disk I/O from `/proc/<PID>/io`.
  - Collects GPU metrics via `nvidia-smi` or AMD sysfs.
  - Provides a generic `RingBuf<T>` plus `ResourceSample` and `detect_fd_leak()` utilities.

- **network-inspector**
  - Reads `/proc/<PID>/fd` to discover socket inodes.
  - Parses `/proc/net/tcp`, `tcp6`, `udp`, `udp6` into TCP/UDP listening sockets and active connections for a given PID.
  - Parses `/proc/net/unix` and correlates with fd inodes to list Unix domain sockets for a PID.
  - Provides a best-effort, time-bounded reverse DNS helper for CLI/TUI display.

- **signal-engine**
  - Counts active TCP connections, child processes, and kernel file locks for a PID.
  - Detects the owning systemd unit from `/proc/<PID>/cgroup`.
  - Produces a `SignalImpact` struct with a human-readable recommendation and a `prefer_graceful` flag.

- **export-engine**
  - Pure formatting crate; depends on `peek-core` for the `ProcessInfo` type and `ProcessSnapshot` wrapper.
  - Exposes:
    - `to_json<T: Serialize>(&T)` → pretty JSON (used with `ProcessSnapshot`).
    - `render_markdown(&ProcessSnapshot)` → Markdown report.
    - `render_html(&ProcessSnapshot)` → dark-themed standalone HTML.
    - `export_pdf(&ProcessSnapshot)` → renders HTML to PDF via wkhtmltopdf/weasyprint/Chromium.

- **peekd**
  - Daemon responsible for history and alerts:
    - `watcher` — sampling loop that calls `peek-core::collect()` on watched PIDs and feeds history + alerts.
    - `ring_store` — in-memory ring-buffer history keyed by PID, with optional JSONL history on disk under `$XDG_STATE_HOME/peekd` or `~/.local/state/peekd` (one `<pid>.jsonl` file per process).
    - `alert` — alert rules and evaluation engine, supporting both dynamic rules from the CLI and static rules loaded from `alerts.toml`.
    - `socket` — Unix domain socket server for the CLI (`peekd_client` in `peek-cli`).

  - IPC JSON messages (peekd ⇄ CLI):
    - Socket path: `/run/peekd/peekd.sock`.
    - All requests are single-line JSON objects with an `"action"` field; responses are `{"ok": bool, "data": ..}` or `{"ok": false, "message": ".."}`.
    - Supported actions:
      - `{"action":"watch","pid":123}` → start sampling PID (returns `{"ok":true,"data":{"watching":123}}`).
      - `{"action":"unwatch","pid":123}` → stop sampling PID and drop its history and rules.
      - `{"action":"list"}` → list watched PIDs: `{"ok":true,"data":{"watching":[...]}}`.
      - `{"action":"history","pid":123}` → returns an array of history samples for the PID; if not in memory, peekd first tries to load `<pid>.jsonl` from disk.
      - `{"action":"alert_add", ...}` → add an alert rule (schema matches `AlertAddRequest` in `crates/peekd/src/alert.rs`); returns `{"ok":true,"data":{"rule_id":"..."}}`.
      - `{"action":"alert_list"}` → list rules: `{"ok":true,"data":{"rules":[...]}}`.
      - `{"action":"alert_remove","rule_id":"..."}"` → remove a rule by ID.
      - `{"action":"ping"}` → health check: `{"ok":true,"data":{"pong":true,"version":"..","watching":N}}`.

