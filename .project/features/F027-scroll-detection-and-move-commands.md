---
id: F027
title: Scroll detection and move commands
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/scroll.rs, web/src/surface/]
---

## Goal
A scroll sends a move command and the new strip only.

## Requirements
- Find vertical moves of a region between two frames
- The client copies the region on its canvas
- Measure the bytes for a scroll of a text page

## Acceptance
