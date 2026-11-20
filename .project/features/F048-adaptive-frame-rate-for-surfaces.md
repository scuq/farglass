---
id: F048
title: Adaptive frame rate for surfaces
status: done
phase: P05
tags: [surface]
decisions: [ADR-007]
files: [crates/farglass-surface/src/pace.rs, crates/farglass-surface/src/state.rs, crates/farglass-session/src/surfaces.rs, crates/farglass-session/src/surface_stream.rs, spikes/e2e/pace.mjs, spikes/e2e/fixtures/animation.py]
---

## Goal
An app with no input does not cost host CPU for frames that the user does not need.

## Requirements
- Frame callbacks at the full rate after input or a window change
- A low rate after 2 s with no input, with a configured value
- A window with no client connected gets no frame callbacks
- A region with video keeps its rate
- Measure the host CPU of an app that draws all the time, with and without the limit

## Acceptance
Full frame rate for 2 s after input or a window change; then the idle rate of FARGLASS_IDLE_FPS (default 10, 0 = off); no frame callbacks for a window with no surface channel; a window with a video region keeps the full rate. pace.mjs: idle 10 fps, input 62 fps, 0 ms app CPU with no browser, video 61 fps idle; host CPU in 4 s idle 300 ms with the limit against 1360 ms without.
