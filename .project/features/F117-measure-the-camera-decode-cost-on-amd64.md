---
id: F117
title: Measure the camera decode cost on amd64
status: planned
phase: P08
tags: [camera]
decisions: [ADR-057]
files: [README.md, spikes/camera/decode.py]
---

## Goal
Know the CPU cost of the camera decode of the agent on an amd64 host, as for arm64.

## Requirements
- Run spikes/camera/decode.py on an amd64 host, and the e2e test npm --prefix spikes/e2e run camera.
- Measure the CPU of farglass-session while a host app reads the camera at 30 pictures each second.
- Write the numbers into the README, next to the arm64 numbers.

## Acceptance
