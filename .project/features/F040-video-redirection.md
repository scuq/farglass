---
id: F040
title: Video redirection
status: done
phase: P07
tags: []
decisions: [ADR-007, ADR-008]
files: [crates/farglass-session/src/redirect.rs, crates/farglass-session/src/launch.rs, web/src/surface/redirect.ts, web/src/surface/worker.ts, web/src/apps/linux/Linux.tsx, extension/redirect/, spikes/e2e/redirect.mjs]
---

## Goal
A player in the host Chromium plays on the client with the hardware decode of the client.

## Requirements
- Only if the spike shows that it works
- The client puts a video element on the rectangle of the player
- Play, pause, and seek stay the same on both sides

## Acceptance
A player in the host Chromium plays in the client browser with its decoder: a video element under the window, with a color key, so the page controls stay on top; play, pause, seek, and rate follow the host within 0.4 s (in the test 0.01 s). The launcher starts Chromium and Brave with the extension, and install-local.sh installs it. Tests: redirect unit tests in Rust and TypeScript, and spikes/e2e/redirect.mjs (local MSE page: the client plays, the host draws black or hides on a black page, pause, seek to 12 s, and play in step) pass; spikes/e2e/youtube.mjs plays a YouTube video the same way.
