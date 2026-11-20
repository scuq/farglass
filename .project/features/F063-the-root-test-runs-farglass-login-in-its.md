---
id: F063
title: The root test runs farglass-login in its systemd unit
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
The root test proves that PAM works with the capability set and the sandbox of farglass-login.service.

## Requirements
- check.sh starts farglass-login with systemd-run and the properties of packaging/systemd/farglass-login.service
- A login, the session start, and the logout pass in that sandbox
- Review finding 17 in docs/threats.md

## Acceptance
