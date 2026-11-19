---
id: F025
title: Lossless tile codec from Wayland damage
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/tiles.rs, web/src/surface/]
---

## Goal
Changed regions go to the client as lossless tiles, and the client draws them.

## Requirements
- Only damaged tiles, from the damage of the Wayland client
- A fast lossless compressor
- A worker decodes and draws on an OffscreenCanvas
- Measure the CPU time for each frame on the host

## Acceptance
