---
id: F068
title: farglassctl users, sessions, PIN, status, and logs
status: done
phase: P06
tags: [packaging]
decisions: [ADR-043, ADR-048]
files: [crates/farglassctl/, crates/farglassd/src/agents.rs, crates/farglassd/src/lock.rs, crates/farglassd/src/server.rs, crates/farglassd/tests/login.rs, README.md]
---

## Goal
The administrator manages the users and the sessions of farglass with farglassctl.

## Requirements
- user list: members of farglass-users, with a web session or not, and passkeys yes or no
- user add USER, user remove USER
- user disable USER: removes the user from farglass-users and ends the web sessions; user enable USER adds the user back
- session list: the web sessions from logind; session logout USER: ends them
- pin set (reads the PIN with no echo), pin clear
- status: the units, the version, the listen address, the certificate, the PIN, the members
- logs [--security] [--follow]: journalctl of the units, or only farglass::security
- doctor: checks of the installation with a hint for each fault
- Tests: the parsers of loginctl, getent, and the log filter

## Acceptance
farglassctl manage.rs: user list (members of farglass-users and the users of /var/lib/farglassctl/disabled, with the web session and the passkey file), user add (usermod -aG), user remove and user disable (gpasswd -d; disable also records the user and ends the web sessions), user enable; session list (loginctl list-sessions and show-session, Type=web Service=farglass-session) and session logout (terminate-session); pin set (no echo, asked two times, through farglassd --set-pin) and pin clear; status; logs with --security (journalctl --grep farglass::security) and --follow; doctor with checks and hints. farglassd: AgentEvent::Gone and agent_gone_loop log out the user of an agent that does not come back in 10 s (cookies removed, tabs get the logout), so a logout from farglassctl shows the login screen. Tests: parsers of getent, show-session, and the listen port; farglassd an_agent_that_stops_and_does_not_come_back_logs_its_user_out. On this host, session list and user list show the web session of temp1. Not tested as root: add, remove, disable, enable, logout, pin, doctor.
