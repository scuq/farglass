---
id: F038
title: Sound from host apps
status: done
phase: P07
tags: []
decisions: [ADR-005]
files: [crates/farglass-session/src/audio.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/desktop.rs, crates/farglass-proto/src/message.rs, web/src/audio/, web/src/panel/Panel.tsx, spikes/e2e/sound.mjs]
---

## Goal
Sound from host apps plays in the client browser.

## Requirements
- A PipeWire sink in the session
- Opus, as in rcd ADR-012
- A jitter buffer in an AudioWorklet

## Acceptance
The agent adds the PipeWire sink farglass, the apps of the web session play to it through PULSE_SINK and PIPEWIRE_NODE, and the agent sends 20 ms Opus packets (opus1, 128 kbit/s, none after 1 s of silence) on an audio channel; the browser decodes them with WebCodecs and plays them through a jitter buffer in an AudioWorklet (60 ms start, 200 ms at most), with a Sound button in the panel. Tests: packet and silence unit tests, a real PipeWire test with pw-cat, the audio channel test, the jitter buffer and panel tests, and spikes/e2e/sound.mjs (a GStreamer tone plays in Chromium at its level) pass.
