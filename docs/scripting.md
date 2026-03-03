# Scripting and automation with peek

`peek` is designed to be script-friendly. This document shows how to use its JSON outputs and `peekd` history in automation, CI, and monitoring.

## JSON outputs

### `--json` (raw `ProcessInfo`)

```bash
peek 1234 --json > process.json
```

This prints a single JSON object representing the current `ProcessInfo` for PID 1234. Example (fields trimmed):

```json
{
  "pid": 1234,
  "name": "nginx",
  "ppid": 1,
  "cmdline": "nginx: worker process",
  "rss_kb": 123456,
  "vm_size_kb": 987654,
  "threads": 12,
  "cpu_percent": 42.3,
  "fd_count": 128,
  "network": {
    "listening_tcp": [ /* ... */ ],
    "connections": [ /* ... */ ]
  }
}
```

Use `jq` to extract fields:

```bash
peek "$PID" --json | jq '.cpu_percent'
peek "$PID" --json | jq '{pid, name, rss_kb, cpu_percent}'
```

### `--json-snapshot` (with metadata)

```bash
peek 1234 --json-snapshot > snapshot.json
```

This wraps `ProcessInfo` with capture metadata:

```json
{
  "captured_at": "2026-03-03T12:34:56Z",
  "peek_version": "1.0.0",
  "process": {
    "pid": 1234,
    "name": "nginx",
    "rss_kb": 123456
    /* ... */
  }
}
```

Useful for storing time-series snapshots in logs or object storage.

## Using `peekd` history programmatically

`peekd` exposes history over a Unix socket as JSON. In most cases you should call it via `peek --history`, but you can also talk to it directly:

```bash
printf '{"action":"history","pid":1234}\n' | socat - UNIX-CONNECT:/run/peekd/peekd.sock
```

The response is a JSON object with a `data.samples` array (one per sample with timestamp, CPU, memory, etc.).

In shell scripts:

```bash
CPU=$(peek "$PID" --json | jq '.cpu_percent // 0')
if [ "$(printf '%.0f' "$CPU")" -gt 80 ]; then
  echo "High CPU for $PID: $CPU%"
fi
```

## CI integration examples

### Fail CI if a test helper leaks FDs

```bash
PID=$(pgrep -n my-test-helper)
FD_BEFORE=$(peek "$PID" --json | jq '.fd_count // 0')

# run expensive tests here...

FD_AFTER=$(peek "$PID" --json | jq '.fd_count // 0')
if [ "$FD_AFTER" -gt "$((FD_BEFORE + 50))" ]; then
  echo "File descriptor leak detected for $PID ($FD_BEFORE -> $FD_AFTER)" >&2
  exit 1
fi
```

### Capture a snapshot artifact on failure

In your CI job:

```bash
if ! cargo test; then
  PID=$(pgrep -n my-server || true)
  if [ -n "$PID" ]; then
    peek "$PID" --json-snapshot > peek-snapshot.json
    # Upload peek-snapshot.json as a CI artifact.
  fi
  exit 1
fi
```

## Tips

- Prefer `--json-snapshot` when you care about capture time and tool version.
- Use `jq -r` to get raw values without quotes for shell conditions.
- Combine `peek --json` with systemd `ExecStartPost` or cron jobs for lightweight health checks.

