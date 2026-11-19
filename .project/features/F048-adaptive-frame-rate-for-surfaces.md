---
id: F048
title: Adaptive frame rate for surfaces
status: planned
phase: P05
tags: [surface]
decisions: [ADR-007]
files: [crates/farglass-surface/]
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
