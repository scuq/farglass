---
id: F019
title: polkit authentication agent
status: done
phase: P04
tags: []
decisions: [ADR-009, ADR-015, ADR-032]
files: [crates/farglass-session/src/polkit.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/main.rs, crates/farglass-proto/src/message.rs, web/src/polkit/, web/src/wm/Desktop.tsx, spikes/, docs/protocol.md, README.md]
---

## Goal
A polkit prompt shows in the shell.

## Requirements
- Register as the authentication agent of the session
- Relay the PAM conversation of polkit to a dialog box in the shell
- Cancel when the client is not connected

## Acceptance
- The agent registers as the authentication agent of its logind session. sudo spikes/session/check.sh confirmed: pkexec /bin/true in a terminal of the web session showed the farglass dialog with the message of polkit and the permitted users.
- The PAM conversation of polkit-agent-helper-1 goes to the dialog: prompts with and without echo, information, failure, and a new attempt. Tests with a fake helper confirm a success, a wrong password and a new attempt, and two tabs.
- Cancel when the client is not connected: with no polkit channel, the request ends at once. When the last channel closes, the request ends. Cancel in the dialog ended pkexec with exit code 126 in the real session.
- Only polkit can call the agent: the agent compares the sender with the owner of org.freedesktop.PolicyKit1.
