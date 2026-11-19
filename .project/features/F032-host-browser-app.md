---
id: F032
title: Host browser app
status: planned
phase: P05
tags: []
decisions: [ADR-008]
files: [crates/farglass-session/src/launch.rs, web/src/apps/]
---

## Goal
Chromium on the host is the web browser of the desktop.

## Requirements
- Start Chromium on the compositor with the Wayland backend
- Links from the terminal and apps can open in it
- Measure the delay and the host CPU while you type and scroll

## Acceptance
