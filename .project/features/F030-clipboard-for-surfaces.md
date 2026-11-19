---
id: F030
title: Clipboard for surfaces
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/clipboard.rs, web/src/surface/]
---

## Goal
Copy and paste between Linux apps, the terminal, and the client computer.

## Requirements
- Wayland data device in the compositor
- Text and PNG
- Async Clipboard API on the client

## Acceptance
