---
name: log-review
description: "Structured log review and diagnosis workflow. Reads latest platform logs, cross-references with DEBUG_DOCUMENT.md, identifies errors and patterns, then enters plan mode for remediation. Use when asked to review logs, debug issues, or check system health."
---

# Log Review & Diagnosis

Structured workflow for reviewing Introvert platform logs and producing an actionable diagnosis. Always enter plan mode after analysis — propose fixes before making changes.

## Prerequisites

- Logs directory: `/Users/dev/Development/introvert/logs/`
- Debug document: `/Users/dev/Development/introvert/DEBUG_DOCUMENT.md`
- Version changelog: `/Users/dev/Development/introvert/VERSION_CHANGELOG.md`

## Steps

### 1. Find Latest Logs

```bash
ls -lt /Users/dev/Development/introvert/logs/ | head -10
```

Identify the most recent log files by platform (android_, macos_, ios_, linux_).

### 2. Read Recent Logs

Read the latest 2–3 log files for each relevant platform. Focus on:
- Error lines (`ERROR`, `error`, `panic`, `FATAL`)
- Warning lines (`WARN`, `warning`)
- Connection state changes (`ConnectionEstablished`, `ConnectionClosed`)
- Transfer state (`FileChunk`, `TransferComplete`, `TransferFailed`)
- Circuit/relay state (`ReservationReq`, `OutboundCircuitEstablished`)

### 3. Cross-Reference Debug Document

```bash
# Read DEBUG_DOCUMENT.md for known issues and status
read /Users/dev/Development/introvert/DEBUG_DOCUMENT.md
```

Check if any errors found in logs are already documented as known issues.

### 4. Check Version Changelog

```bash
# Read recent changelog entries
read /Users/dev/Development/introvert/VERSION_CHANGELOG.md
```

Verify if errors relate to recent changes.

### 5. Compile Findings

Produce a structured summary:
- **Platform(s) affected:** (macOS, Android, iOS, Linux, RBN)
- **Error count:** total errors found
- **Critical issues:** errors that block functionality
- **Warnings:** degraded but functional
- **Patterns:** recurring errors, timing correlations
- **Known vs new:** which are documented, which are new

### 6. Enter Plan Mode

After completing the review, enter plan mode to propose remediation steps. Do not make code changes during the review — diagnose first, then plan.

## Common Error Patterns

| Pattern | Likely Cause |
|---------|-------------|
| `ConnectionClosed` + `FileTransfer` stuck | VPN disruption or relay circuit flap |
| `PendingOutgoing` exhaustion | Too many queued transfers, backpressure needed |
| `gossipsub` publish failure | Peer not connected, needs relay fallback |
| `is_connected` returns false for cross-network | Expected — cross-network peers use relay |
| `systemctl is-active` fails on RBN | Daemon crash, check OOM / segfault |
| `cargo check` errors after restore | Missing dependencies or version mismatch |

## Rules

- Enter plan mode after reading logs — user directive.
- Never delete log files without explicit user confirmation.
- RBN logs require SSH to `root@47.89.252.80` — check remote logs separately.
