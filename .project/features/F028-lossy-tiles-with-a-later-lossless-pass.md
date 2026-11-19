---
id: F028
title: Lossy tiles with a later lossless pass
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/classify.rs, web/src/surface/]
---

## Goal
Images and gradients go as lossy tiles first and become exact when they stop.

## Requirements
- Classify each tile as text or image by its colors
- Lossy codec that the browser decodes natively
- Lossless pass for regions that do not change, as in rcd ADR-006

## Acceptance
