---
id: ADR-026
title: Terminals live in the agent with an id, flow control by byte count, and a scope each
status: accepted
date: 2026-11-19
tags: [terminal, protocol]
superseded_by:
files: [crates/farglass-session/src/terminal.rs, crates/farglass-session/src/link.rs, crates/farglass-proto/src/message.rs]
---

## Context
A terminal must continue when the tab closes, and the scrollback must be there after the shell connects again (phase P03). One agent link carries all tabs of a user, thus a slow tab can fill the queues (ADR-017). Each terminal must run in its own systemd user scope.

## Decision
- The shell gives each terminal an id. A pty channel with a known id attaches, and a new id starts the login shell on a new pty. A close frame only detaches. PtyRequest::Terminate ends the terminal.
- Each terminal is a task that owns the pty, the scrollback (last 1 MiB), the exit status, and the attached channels. A separate writer task writes the input and the size changes, thus a program that does not read its input cannot stop the output.
- Flow control: the task reads the pty only while each attached channel has less than PTY_WINDOW (256 KiB) of output that the shell did not acknowledge. The shell acknowledges with PtyRequest::Ack. With no attached channel, the task reads with no limit.
- An attach sends PtyEvent::Attached with the size of the replay, then the scrollback, then the live output. More than one channel can attach to one terminal.
- With a systemd user manager, the program starts through systemd-run --user --scope as farglass-terminal-<random>.scope. Terminate sends SIGHUP to each process of the scope with systemctl --user kill, and SIGKILL after 3 seconds. With no manager, the signals go to the process group.
- If the program stops with no attached channel, the terminal keeps its scrollback and exit status for 24 hours, until the next attach.
- The state store and the terminals are one Session, which lives as long as the agent, also when the link to farglassd breaks. At most 64 terminals.

## Consequences
The terminal app (F015) uses the window id as the terminal id, acknowledges after xterm.js wrote the output, and sends terminate only when its window closes. The scrollback is raw output: a replay can start in the middle of an escape sequence, thus the app resets the terminal before the replay. Terminals end with the agent, thus a logout ends them.
