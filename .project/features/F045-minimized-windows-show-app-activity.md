---
id: F045
title: Minimized windows show app activity
status: done
phase: P03
tags: [shell]
decisions: [ADR-001]
files: [crates/farglass-session/src/stats.rs, crates/farglass-surface/src/state.rs, crates/farglass-session/src/terminal.rs, web/src/stats.ts, web/src/panel/]
---

## Goal
A minimized window shows how busy its app is.

## Requirements
- The agent reads CPU, memory, and network of the systemd scope of each app
- The panel shows a busy app and an idle app differently
- Details show when the pointer is on the item
- The agent sends numbers only while a client is connected

## Acceptance
The agent reads CPU and memory of the systemd scope of each Linux app window and terminal (or the process and its children outside a scope) and the TCP traffic of its processes from ss, every 2 s and only while a stats channel is open; the panel button of a minimized window pulses for a busy app and is hollow for an idle one, and its tooltip gives CPU, memory, and network. Tests: stats unit tests, the stats channel test with a busy loop in a real terminal (over 50 % CPU), and the panel activity test pass.
