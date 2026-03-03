# Security

## Reporting a vulnerability

If you believe you have found a security vulnerability in peek or peekd, please report it responsibly.

**Do not** open a public GitHub issue for security-sensitive findings.

**Preferred:** Email the maintainers (see [GitHub repo](https://github.com/ankittk/peek) for contact) with a description of the issue, steps to reproduce, and any suggested fix if you have one. We will acknowledge and work with you to understand and address the report.

**What to include:**

- Affected component (peek CLI, peekd, or a specific crate)
- Description of the vulnerability and impact
- Steps to reproduce
- Your environment (OS, version)

We will respond as quickly as we can and will credit you in any advisory unless you prefer to remain anonymous.

## Scope

- **In scope:** peek and peekd codebase, install scripts, and packaging that we ship.
- **Out of scope:** Third-party tools (e.g. wkhtmltopdf, weasyprint) used for PDF export; general OS or kernel issues.

## Security considerations

- peek and peekd read from `/proc` and system interfaces; they are intended to be run by users with permission to inspect the target processes.
- peekd can run as root so it can sample any PID; the systemd unit uses root by default. For same-UID monitoring only, you can run peekd as a normal user or use `DynamicUser`.
- The daemon listens on a Unix socket (`/run/peekd/peekd.sock`); by default the socket is restricted to owner and group (0o660). Use systemd unit settings or filesystem ACLs if you need to relax or further restrict access. There is no additional authentication beyond Unix socket permissions.
