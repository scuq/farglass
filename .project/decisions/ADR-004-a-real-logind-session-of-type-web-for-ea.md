---
id: ADR-004
title: A real logind session of type web for each user
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
The desktop must behave as GNOME or KDE does for sessions and security: PAM, logind, a user manager, cgroups, and audit records.

## Decision
farglass-login relays the PAM conversation to the login screen. It starts a transient unit that opens a PAM session with pam_systemd, class user, type web, changes to the user, and starts farglass-session. One session for each user. The session continues when the tab closes. An idle session logs out after a configured time.

## Consequences
All PAM methods work. loginctl shows the session. Terminals continue to run without a client. The agent must keep the state that the shell needs when it connects again.
