---
id: F025
title: Lossless tile codec from Wayland damage
status: done
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/tiles.rs, crates/farglass-session/src/surface_stream.rs, web/src/surface/, web/src/apps/linux/, spikes/e2e/surface.mjs]
---

## Goal
Changed regions go to the client as lossless tiles, and the client draws them.

## Requirements
- Only damaged tiles, from the damage of the Wayland client
- A fast lossless compressor
- A worker decodes and draws on an OffscreenCanvas
- Measure the CPU time for each frame on the host

## Acceptance
Each surface has a channel with a binary tile stream (hello, frame, tile, fill, end). The encoder in farglass-surface looks only at the 64x64 tiles in the Wayland damage, skips tiles that equal what the client shows, sends one-color tiles as fills and the rest as LZ4 of BGRA. Waiting commits merge their damage, and the encoder runs on a blocking thread. In the shell, a worker decodes the stream (with partial messages across data frames) and draws on an OffscreenCanvas. The surface manager opens a window of the hidden linux app for each mapped top-level window, crops the client shadow with the geometry, places popups, and sends configure, activate, and close. Tested by Rust tests with a reference decoder (exact pixels, only changed tiles, new size), vitest tests (LZ4, premultiplied alpha, split messages), and spikes/e2e/surface.mjs: gnome-calculator shows in Chromium with 421 colors, a maximize gives a 1402 px buffer, the close button stops the app, and the encoder logs its CPU time per frame (24 ms for a full 482x738 frame in a debug build).
