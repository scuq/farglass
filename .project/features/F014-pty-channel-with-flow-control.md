---
id: F014
title: pty channel with flow control
status: planned
phase: P03
tags: []
decisions: [ADR-005]
files: [crates/farglass-session/src/pty.rs]
---

## Goal
The agent runs terminals that continue with no client.

## Requirements
- Each terminal in its own systemd --user scope
- Flow control by byte count
- Size changes
- A scrollback buffer that the shell gets when it connects again

## Acceptance
