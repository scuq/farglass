---
id: F020
title: Notifications
status: planned
phase: P04
tags: []
decisions: [ADR-001]
files: [crates/farglass-session/src/notify.rs, web/src/]
---

## Goal
Desktop notifications from host programs show in the shell.

## Requirements
- The agent owns org.freedesktop.Notifications
- Actions and close events go back to the sender
- Notifications wait while no client is connected

## Acceptance
