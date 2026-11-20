---
id: F063
title: The root test runs farglass-login in its systemd unit
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [spikes/session/check.sh, README.md, docs/threats.md]
---

## Goal
The root test proves that PAM works with the capability set and the sandbox of farglass-login.service.

## Requirements
- check.sh starts farglass-login with systemd-run and the properties of packaging/systemd/farglass-login.service
- A login, the session start, and the logout pass in that sandbox
- Review finding 17 in docs/threats.md

## Acceptance
check.sh starts farglass-login with systemd-run as the transient unit farglass-check-login, with each property of the [Service] section of packaging/systemd/farglass-login.service except Type, ExecStart, and User, plus ReadWritePaths for the test socket directory; the logs come from journalctl. sh -n passes. The run as root (sudo spikes/session/check.sh) is the test: a login, the session start, and the logout in that sandbox.
