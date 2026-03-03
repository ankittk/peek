# FAQ

## Do I need root to use peek and peekd?

- Many features (basic process info, TUI, JSON export) work without root.
- Some data under `/proc` (open files, environment, cgroups, history for other UIDs) may require `sudo`.
- For system-wide monitoring, run `peekd` as root via the packaged systemd unit.

## Why doesn’t feature X work on macOS or Windows?

`peek` targets Linux first. On macOS and Windows only a subset is available:

- Basic process info and TUI/export work.
- Kernel, network, open files, env, process tree, and `peekd` are Linux-only because they rely on `/proc` and Linux-specific APIs.

See the “Platform support” table in `README.md` for the current matrix.

## Can I use peek inside containers or Docker?

- Yes, but visibility is limited by what `/proc` exposes inside the container.
- For best results, run `peek` / `peekd` on the host and inspect container processes by PID or name (e.g. `dockerd`, `containerd-shim`, or the container’s main process).

## Where does peekd store history on disk?

History is stored as JSONL under:

1. `$XDG_STATE_HOME/peekd` if set, else
2. `$HOME/.local/state/peekd`, else
3. `/var/lib/peekd`.

Each PID gets a separate `<pid>.jsonl` file. The in-memory ring size (and on-disk window) is controlled by `PEEKD_RING_SIZE`.

## How do I configure alert rules?

- Static rules: put an `alerts.toml` in one of:
  - `$XDG_CONFIG_HOME/peek/alerts.toml`
  - `$HOME/.config/peek/alerts.toml`
  - `/etc/peekd/alerts.toml`
  - `/etc/peek/alerts.toml`
- Dynamic rules: use the `peek` CLI:

```bash
peek 1234 --alert-add cpu_percent gt 80
peek --alert-list
peek --alert-remove <RULE_ID>
```

See `docs/peekd.md` for the full TOML schema and notification options.

## What’s the difference between `--json` and `--json-snapshot`?

- `--json` prints the raw `ProcessInfo` for a single point in time.
- `--json-snapshot` wraps that in `{ captured_at, peek_version, process }`, which is better for logs and long-term storage.

