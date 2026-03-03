# peekd daemon

`peekd` is the background daemon that powers `peek --history` and alert rules.

- Samples watched processes every second into an in-memory ring buffer (with optional JSONL history on disk under `$XDG_STATE_HOME/peekd` or `~/.local/state/peekd`).
- Listens on a Unix domain socket (`/run/peekd/peekd.sock`) for JSON requests from the `peek` CLI.
- Evaluates alert rules from a static `alerts.toml` plus dynamic rules added via the CLI.

## Running peekd

- **Systemd (recommended):** install the packaged `peekd.service` unit and enable it:

```bash
sudo systemctl enable --now peekd
```

- **Foreground (debugging):**

```bash
peekd
```

Logs go to stderr and to the systemd journal when run via the unit. Use `RUST_LOG=debug` to increase verbosity.

## Unix socket protocol

- Socket path: `/run/peekd/peekd.sock`.
- All requests are single-line JSON objects with an `"action"` field.
- Responses are:
  - `{"ok": true, "data": ...}` on success
  - `{"ok": false, "message": "error text"}` on failure

Supported actions:

- `{"action":"watch","pid":123}` → start sampling PID.
- `{"action":"unwatch","pid":123}` → stop sampling and drop history + rules for PID.
- `{"action":"list"}` → list watched PIDs.
- `{"action":"history","pid":123}` → return recent samples for PID.
- `{"action":"alert_add", ...}` → add an alert rule (see below).
- `{"action":"alert_list"}` → list alert rules.
- `{"action":"alert_remove","rule_id":"..."}"` → remove a rule by ID.
- `{"action":"ping"}` → health check (`{"pong":true,"version":"..","watching":N}`).

## Alert rules (static config)

`peekd` can load rules from an `alerts.toml` file. Search order:

1. `$XDG_CONFIG_HOME/peek/alerts.toml`
2. `$HOME/.config/peek/alerts.toml`
3. `/etc/peekd/alerts.toml`
4. `/etc/peek/alerts.toml`

Example `alerts.toml`:

```toml
[[rules]]
pid = 1234
metric = "cpu_percent"      # or: "memory_mb", "fd_count", "thread_count"
comparison = "greater_than" # or: "less_than"
threshold = 80.0
cooldown_secs = 60          # optional, default 60
notify = { type = "log" }   # or: { type = "stderr" } or { type = "script", command = "echo {pid} {metric} {value}" }
```

On startup, `peekd` loads all rules from `alerts.toml`, seeds the watched PID set, and logs where the file was read from.

## Alert rules (dynamic via CLI)

The `peek` CLI can add or remove rules at runtime (backed by the same engine in `peekd`):

```bash
peek 1234 --alert-add cpu_percent gt 80   # metric, comparison, threshold
peek --alert-list                         # list rules
peek --alert-remove <RULE_ID>             # remove rule
```

Dynamic rules are stored in memory only; restart `peekd` to get back to the static config baseline.

## History storage

- In-memory: bounded ring buffer per PID (default 5 minutes at 1s resolution; tunable via `PEEKD_RING_SIZE`).
- On disk: JSONL per PID under:
  - `$XDG_STATE_HOME/peekd` if set, else
  - `$HOME/.local/state/peekd`, else
  - `/var/lib/peekd`.

The `peek --history` command asks `peekd` for recent samples and renders sparklines and tables in the CLI.

