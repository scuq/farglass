---
id: F029
title: Surface input, local cursor, and frame acknowledgment
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/input.rs, web/src/surface/]
---

## Goal
Input has no delay other than the network, and a slow network gets the newest state only.

## Requirements
- Keyboard and pointer events as Wayland seat events
- The client draws the cursor from a shape message
- Input on its own stream
- The client acknowledges frames, and the host drops old state

## Acceptance
