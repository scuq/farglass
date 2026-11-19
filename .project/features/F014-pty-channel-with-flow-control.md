---
id: F014
title: pty channel with flow control
status: done
phase: P03
tags: []
decisions: [ADR-005, ADR-017, ADR-022, ADR-026]
files: [crates/farglass-session/, crates/farglass-proto/src/message.rs, crates/farglass-proto/src/lib.rs, web/src/proto/generated/, docs/protocol.md]
---

## Goal
The agent runs terminals that continue with no client.

## Requirements
- Each terminal in its own systemd --user scope
- Flow control by byte count
- Size changes
- A scrollback buffer that the shell gets when it connects again

## Acceptance
- Each terminal runs in its own systemd --user scope farglass-terminal-*.scope. A test with the user manager confirms it from /proc/self/cgroup, and that terminate removes the scope.
- Flow control by byte count: with no acknowledgment, the agent sent at most the window of 256 KiB plus one read of 32 KiB from 4 MB of output. The acknowledgments let the rest come.
- Size changes: stty size shows 24 80 at the start and 50 132 after a resize message.
- The scrollback (last 1 MiB) comes first at each attach. Tests confirm: the same shell with its variables after a reattach, a job that wrote output while no channel was attached, a terminal after a break of the link to farglassd, and the exit of a program while no channel was attached.
- exit and terminate end the terminal with the exit code or the signal. The 8 terminal tests passed in 6 runs.
