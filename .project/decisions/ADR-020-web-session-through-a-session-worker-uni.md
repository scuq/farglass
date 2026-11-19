---
id: ADR-020
title: Web session through a session worker unit, and host actions behind an interface
status: accepted
date: 2026-11-19
tags: [session, login]
superseded_by:
files: [crates/farglass-login/src/host.rs, crates/farglass-login/src/worker.rs, packaging/pam.d/farglass-session, spikes/session/check.sh]
---

## Context
ADR-004 needs a real logind session of type web for each user. ADR-015 needs the end of the local graphical sessions after a confirm. These actions need root, and most tests run with no root.

## Decision
- After PAM and the account checks, farglass-login asks logind for the local graphical sessions of the user (class user, type x11, wayland, or mir, not closing). If there are any, it sends LoginEvent::Confirm and waits up to 60 seconds for LoginRequest::Confirm. A decline, a timeout, or a closed connection ends nothing and does not count for the rate limiter.
- After a confirm, farglass-login ends the sessions with loginctl terminate-session and waits up to 20 seconds.
- A web session of the user is a logind session with type web and the PAM service farglass-session. If one is open, the login attaches. If not, farglass-login runs systemd-run --unit=farglass-session-UID with farglass-login --session-worker USER.
- The worker opens the PAM session farglass-session with XDG_SESSION_TYPE=web and XDG_SESSION_CLASS=user, ignores SIGTERM and SIGHUP, starts the agent as the user, and closes the PAM session after the agent stops.
- farglassd sends success to the shell after the agent of the user connected, at most 30 seconds later.
- All host actions go through the trait Host. Systemd does them with loginctl and systemd-run. NoSessions (--session none) and the test hosts do nothing.
- spikes/session/check.sh tests the real session with root and a temporary user. It never ends a local session of the user who runs it.

## Consequences
The unit tests and the end-to-end tests need no root. Each change to the session code needs a run of spikes/session/check.sh with sudo. Logout (F010) ends the logind session, and the worker then closes the PAM session. A local login during a web session is not blocked, because farglass does not control the display manager.
