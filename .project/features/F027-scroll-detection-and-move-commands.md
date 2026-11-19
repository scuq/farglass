---
id: F027
title: Scroll detection and move commands
status: done
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/scroll.rs, crates/farglass-surface/src/tiles.rs, web/src/surface/decoder.ts, web/src/surface/worker.ts]
---

## Goal
A scroll sends a move command and the new strip only.

## Requirements
- Find vertical moves of a region between two frames
- The client copies the region on its canvas
- Measure the bytes for a scroll of a text page

## Acceptance
The encoder hashes each row of the damaged columns in the old and the new frame, takes the vertical offset with the most votes (rows of one color give none), and sends a move for the largest block of rows that moved, at least 24 rows. After the move, the tile diff sends only the new strip; each tile in the damage box counts as dirty. The worker copies the block with getImageData and putImageData. Measured in a Rust test: a scroll of 53 rows of an 800x600 text page costs 28753 bytes with the move and 186256 bytes without, and the canvas of the reference decoder has the exact new frame. A vitest test checks the move in the TS decoder.
