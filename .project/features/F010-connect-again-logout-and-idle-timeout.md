---
id: F010
title: Connect again, logout, and idle timeout
status: done
phase: P02
tags: []
decisions: [ADR-004, ADR-017, ADR-021, ADR-022]
files: [crates/farglass-session/, crates/farglassd/, crates/farglass-login/, crates/farglass-proto/, web/src/, spikes/, docs/protocol.md, packaging/farglassd.conf]
---

## Goal
The session continues with no client and ends on logout or timeout.

## Requirements
- The agent keeps the list of open channels and windows
- The shell restores its state after it connects again
- Logout ends the session unit
- Idle logout after a configured time

## Acceptance
- The agent keeps a state store for the session. Its values continue after a reload, a break of the connection, and a restart of the agent link. F011 keeps the window list there. The reattach of terminal channels comes with F014. Tests confirm the store, the change notices, and the limits.
- After a break, the shell connects again with backoff and with no login. Chromium through a TCP relay that the test cut confirmed it, and the echo log came back from the state store after a reload.
- Log out ends the session unit. sudo spikes/session/check.sh confirmed: after Log out, loginctl showed no web session of the test user, and its agent stopped. Each other tab of the user gets the logout and shows the login screen.
- The idle logout ends a session whose user had no browser connection for idle_logout_minutes (default 480, 0 turns it off). Tests confirm that a user with a tab is not idle and a user with no tab is logged out.
