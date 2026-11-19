---
id: F028
title: Lossy tiles with a later lossless pass
status: done
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/classify.rs, crates/farglass-surface/src/tiles.rs, crates/farglass-session/src/surface_stream.rs, web/src/surface/decoder.ts, web/src/surface/worker.ts]
---

## Goal
Images and gradients go as lossy tiles first and become exact when they stop.

## Requirements
- Classify each tile as text or image by its colors
- Lossy codec that the browser decodes natively
- Lossless pass for regions that do not change, as in rcd ADR-006

## Acceptance
A tile with more than 256 distinct colors counts as an image and goes as a JPEG (quality 70, jpeg-encoder from BGRA), which the browser decodes natively with createImageBitmap; the decoder waits for each JPEG before the next message. Lossy tiles take no cache slot. The encoder keeps the time of each lossy tile, moves the marks with a scroll, and 300 ms after a tile stops changing the stream sends it again, exact (the lossless pass of rcd ADR-006). Tested by Rust tests (4 photo tiles and 4 text tiles: 23918 bytes with JPEG, 77842 lossless; exact canvas after the pass; a new change starts the time again), a vitest test of the JPEG order, and spikes/e2e/surface.mjs with eog showing a photo: 164 lossy tiles, then 164 tiles made exact.
