---
id: F114
title: Microphone from the client to host apps
status: planned
phase: P08
tags: [audio, protocol]
decisions: [ADR-014, ADR-017, ADR-036]
files: [crates/farglass-session/src/mic.rs, crates/farglass-session/src/audio.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/desktop.rs, crates/farglass-proto/src/message.rs, web/src/audio/, web/src/panel/Panel.tsx, packaging/farglassd.conf, spikes/e2e/mic.mjs, README.md, CHANGELOG.md]
---

## Goal
A host app records the microphone of the client.

## Requirements
- A new channel kind, mic, from the shell to the agent. The shell sends the format, then one data frame for each packet: the packet number as u32, little-endian, then one Opus packet. This is the form of the audio channel (F038) in the opposite direction.
- The shell captures with getUserMedia, with echo cancellation, noise suppression, and automatic gain of the browser on. The sound of the host plays on the client, thus the client is the place to cancel the echo.
- The shell encodes with the WebCodecs AudioEncoder: Opus, 48 kHz, mono, 20 ms, 32 kbit/s, with DTX.
- The agent adds the PipeWire source farglass-mic (media.class Audio/Source) and decodes with opus1. The desktop variables name it in PULSE_SOURCE. With no channel, the source gives silence.
- A small jitter buffer in the agent: 40 ms start, 120 ms at most. Opus loss concealment fills a lost packet.
- The microphone is off after each connect. A Microphone button in the panel, next to Sound, turns it on and off. The button shows the three states: off, on, and refused by the browser.
- One connection of a user holds the microphone (ADR-017). The connection that turns it on last gets it, and the other connection shows off.
- A lock, a logout, and a lost connection stop the capture and release the device in the browser.
- The agent refuses a packet that is too large and a packet rate above the format. The start and the stop go to the security log (ADR-043).
- A key microphone in packaging/farglassd.conf, default true. With false, farglassd refuses the channel and the panel shows no button. Update the README quick start and the changelog in the same edit.
- Chromium is the target (ADR-006). A browser with no AudioEncoder shows no button.

## Acceptance
