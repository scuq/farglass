---
id: F040
title: Video redirection
status: planned
phase: P07
tags: []
decisions: [ADR-007, ADR-008]
files: [crates/farglass-session/, web/src/surface/]
---

## Goal
A player in the host Chromium plays on the client with the hardware decode of the client.

## Requirements
- Only if the spike shows that it works
- The client puts a video element on the rectangle of the player
- Play, pause, and seek stay the same on both sides

## Acceptance
