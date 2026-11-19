---
id: F037
title: Video codec for regions that change all the time
status: planned
phase: P07
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/video.rs, web/src/surface/]
---

## Goal
A region with video uses a video codec for that rectangle only.

## Requirements
- Find a region that changes in each frame
- H.264 or AV1, with VAAPI when the host has it and in software when it does not
- WebCodecs decode in a worker
- Tiles for the rest of the window

## Acceptance
