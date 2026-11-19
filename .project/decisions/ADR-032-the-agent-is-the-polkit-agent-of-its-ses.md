---
id: ADR-032
title: The agent is the polkit agent of its session and relays the helper to a polkit channel
status: accepted
date: 2026-11-19
tags: [polkit, security]
superseded_by:
files: [crates/farglass-session/src/polkit.rs, web/src/polkit/]
---

## Context
ADR-009 has no root bridge: settings change system services through D-Bus, and polkit asks the user. polkit needs an authentication agent for the session. The terminals run in scopes of the user manager, outside the logind session. A process on the system bus can call an agent object.

## Decision
- At its start, the agent registers with RegisterAuthenticationAgent for the unix-session of XDG_SESSION_ID and exports org.freedesktop.PolicyKit1.AuthenticationAgent at /org/farglass/PolkitAgent.
- BeginAuthentication and CancelAuthentication answer only the unique name that owns org.freedesktop.PolicyKit1.
- Identities: each unix-user, and each member of each unix-group. The first attempt uses the user of the agent if polkit permits it.
- Each attempt runs /usr/lib/polkit-1/polkit-agent-helper-1 USER with the cookie on its first line, and relays its PAM_PROMPT_ECHO_OFF, PAM_PROMPT_ECHO_ON, PAM_TEXT_INFO, PAM_ERROR_MSG, SUCCESS, and FAILURE lines. The helper tells polkit the result.
- The shell keeps a polkit channel open on the desktop. Each request goes to each channel of the user. Any tab can answer. Each tab gets the end.
- The agent cancels a request with no channel, after the last channel closes, after 5 minutes, on a cancel of the user, and on CancelAuthentication.
- The dialog is a modal <dialog>. After a failure, it starts a new attempt at once. Its logic uses plain variables, because events come in one task and Solid commits signals later.

## Consequences
logind elects the web session as the display session of the user (confirmed: Display=441), thus pkexec in a terminal outside the session finds the agent. A test of the real path needs root (spikes/session/check.sh). If a user also has a graphical session, logind elects that one, but ADR-015 ends it at the web login. The settings (F022) use the interactive flag of the dbus channel.
