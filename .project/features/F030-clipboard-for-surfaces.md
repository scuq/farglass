---
id: F030
title: Clipboard for surfaces
status: done
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/clipboard.rs, crates/farglass-surface/src/state.rs, crates/farglass-session/src/surfaces.rs, crates/farglass-session/src/link.rs, crates/farglass-proto/src/message.rs, web/src/surface/, web/src/apps/linux/Linux.tsx, spikes/e2e/clipboard.mjs]
---

## Goal
Copy and paste between Linux apps, the terminal, and the client computer.

## Requirements
- Wayland data device in the compositor
- Text and PNG
- Async Clipboard API on the client

## Acceptance
A copy in a Wayland app reaches the browser clipboard, and a paste key first gives the browser clipboard to the apps; text and PNG on a clipboard channel. Tests: the compositor test with a real wl_data_device client both ways, the link test with a 300 KiB PNG in several frames, the bridge tests, and spikes/e2e/clipboard.mjs (Chromium to VS Code, VS Code to Chromium, gnome-calculator to Chromium and VS Code) pass.
