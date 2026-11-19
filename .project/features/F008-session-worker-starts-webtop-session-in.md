---
id: F008
title: Session worker starts farglass-session in a logind session
status: planned
phase: P02
tags: []
decisions: [ADR-004, ADR-015]
files: [crates/farglass-login/src/worker.rs, packaging/pam.d/farglass-session]
---

## Goal
A login gives a real session of type web that runs the agent as the user.

## Requirements
- Transient unit with a PAM session through pam_systemd, class user, type web
- Start from rcd/crates/rcdd/src/worker.rs
- One session for each user. A second login attaches to it
- Before the session starts, end the local graphical sessions of the user through logind, after the user confirms (ADR-015)
- A confirm event on the login channel and a confirm step in the login screen: the local session will end, and work that is not saved will be lost (ADR-015)
- Web sessions of different users run at the same time
- loginctl shows the session

## Acceptance
