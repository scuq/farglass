---
id: F038
title: Sound from host apps
status: planned
phase: P07
tags: []
decisions: [ADR-005]
files: [crates/farglass-session/src/audio.rs, web/src/]
---

## Goal
Sound from host apps plays in the client browser.

## Requirements
- A PipeWire sink in the session
- Opus, as in rcd ADR-012
- A jitter buffer in an AudioWorklet

## Acceptance
