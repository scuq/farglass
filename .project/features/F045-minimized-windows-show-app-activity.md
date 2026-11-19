---
id: F045
title: Minimized windows show app activity
status: planned
phase: P03
tags: [shell]
decisions: [ADR-001]
files: [web/src/panel/, crates/farglass-session/src/stats.rs]
---

## Goal
A minimized window shows how busy its app is.

## Requirements
- The agent reads CPU, memory, and network of the systemd scope of each app
- The panel shows a busy app and an idle app differently
- Details show when the pointer is on the item
- The agent sends numbers only while a client is connected

## Acceptance
