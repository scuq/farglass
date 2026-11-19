---
id: F008
title: Session worker starts farglass-session in a logind session
status: done
phase: P02
tags: []
decisions: [ADR-004, ADR-015, ADR-018, ADR-020]
files: [crates/farglass-login/, crates/farglassd/src/login.rs, crates/farglass-proto/src/login.rs, web/src/, packaging/pam.d/farglass-session, spikes/session/, spikes/e2e/, docs/protocol.md]
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
- farglass-login starts a transient unit farglass-session-UID with the session worker. The worker opens a PAM session with pam_systemd, class user, type web, and runs the agent as the user. spikes/session/check.sh with root confirmed: loginctl shows Type=web, Class=user, Service=farglass-session, State=active, and the agent runs as the test user and answers on the echo channel.
- A second login of the same user attached to the same session. check.sh confirmed it.
- A user with a local graphical session gets a confirm event. The login screen shows the session, puts the focus on Cancel, and names the loss of work that is not saved. check.sh confirmed: Cancel stops the login, the local Wayland session continues, and no web session starts.
- After a confirm, farglass-login ends the local sessions through logind before the web session starts. Tests with a fake host confirm the sequence.
- Web sessions of different users run at the same time: one unit and one agent for each uid.
- farglassd sends success after the agent connected. A test with an agent that connects 300 ms later confirms it.
