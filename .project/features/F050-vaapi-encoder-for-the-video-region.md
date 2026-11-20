---
id: F050
title: VAAPI encoder for the video region
status: planned
phase: P07
tags: [optional]
decisions: [ADR-007]
files: [crates/farglass-surface/src/video.rs]
---

## Goal
The video region uses the hardware H.264 encoder of the host through VAAPI when the host has one, and OpenH264 when it does not.

## Requirements
- Find a VAAPI device with an H.264 encode entry point at the start of a video
- The same stream format as OpenH264: Annex B, a key frame at the start
- A test on a host with VAAPI; the first test host has no libva

## Acceptance
