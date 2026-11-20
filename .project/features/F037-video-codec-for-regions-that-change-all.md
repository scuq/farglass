---
id: F037
title: Video codec for regions that change all the time
status: done
phase: P07
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/video.rs, crates/farglass-surface/src/tiles.rs, web/src/surface/, web/src/apps/linux/Linux.tsx, spikes/e2e/video.mjs, spikes/e2e/fixtures/animation.py]
---

## Goal
A region with video uses a video codec for that rectangle only.

## Requirements
- Find a region that changes in each frame
- H.264 or AV1, with VAAPI when the host has it and in software when it does not
- WebCodecs decode in a worker
- Tiles for the rest of the window

## Acceptance
A block of at least 6 tiles that changes in 12 of the last 16 commits within 1.6 s goes as H.264 (OpenH264, software) while the rest of the window stays tiles; the browser decodes it with WebCodecs in the worker; after 500 ms with no change the region goes as exact tiles. Tests: detector and region unit tests, the tile encoder test that decodes the H.264 with a mean error below 12 and ends exact, the cursor and typing test with no video, the decoder tests, and spikes/e2e/video.mjs (about 60 decoded frames each second from a GTK 4 animation) pass. VAAPI moved to F050: the test host has no libva.
