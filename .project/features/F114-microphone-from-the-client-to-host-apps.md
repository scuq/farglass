---
id: F114
title: Microphone from the client to host apps
status: done
phase: P08
tags: [audio, protocol]
decisions: [ADR-014, ADR-017, ADR-036]
files: [crates/farglass-session/src/mic.rs, crates/farglass-session/src/audio.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/desktop.rs, crates/farglass-session/src/main.rs, crates/farglass-session/tests/mic.rs, crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, crates/farglassd/src/config.rs, web/src/audio/, web/src/panel/Panel.tsx, web/src/panel/panel.css, web/src/wm/Desktop.tsx, packaging/farglassd.conf, spikes/e2e/mic.mjs, docs/protocol.md, README.md, CHANGELOG.md]
---

## Goal
A host app records the microphone of the client.

## Requirements
- A new channel kind, mic, between the shell and the agent. The agent sends the format at once (ready, or unavailable). The shell sends start and stop. After a start, each data frame holds one packet: the packet number as u32, little-endian, then one Opus packet.
- The shell captures with getUserMedia, with echo cancellation, noise suppression, and automatic gain of the browser on. The sound of the host plays on the client, thus the client is the place to cancel the echo.
- The shell encodes with the WebCodecs AudioEncoder: Opus, 48 kHz, mono, 20 ms, 32 kbit/s, with DTX.
- The agent adds the PipeWire source farglass-mic (media.class Audio/Source) and decodes with opus1. The desktop variables name it in PULSE_SOURCE. With no channel, the source gives silence.
- A small jitter buffer in the agent: 40 ms start, 120 ms at most. Opus loss concealment fills a lost packet.
- The microphone is off after each connect. A Mic button in the panel, next to Sound, turns it on and off. The button shows the three states: off, on, and refused by the browser.
- One connection of a user holds the microphone (ADR-017). The connection that turns it on last gets it, and the other connection gets taken and shows off.
- A lock, a logout, and a lost connection stop the capture and release the device in the browser.
- The agent refuses a packet that is too large and a packet rate above the format. The start and the stop go to the security log (ADR-043).
- A key microphone in packaging/farglassd.conf, default true. The agent reads it, as for x11_apps and ssh_agent. With false, the agent starts no source and refuses the channel, and the panel shows no button.
- Chromium is the target (ADR-006). A browser with no AudioEncoder shows no button.

## Acceptance
The agent adds the PipeWire source farglass-mic (Audio/Source, mono, 48 kHz) and names it in PULSE_SOURCE. A mic channel gets ready with the format at once, or unavailable with no source; with microphone = false the agent starts no source and refuses the channel with access-denied. After start, the shell sends 20 ms Opus packets (WebCodecs AudioEncoder, 32 kbit/s, DTX) from getUserMedia with echo cancellation, noise suppression, and automatic gain, cut into blocks by an AudioWorklet. The agent decodes them with opus1 into a jitter buffer (40 ms start, 120 ms at most, concealment for a gap of up to 5 packets), clamps to full scale, and refuses a packet before start, a wrong size, or more than 50 packets each second after a 150-packet burst; start, stop, and refusals go to farglass::security. The last channel that starts holds the microphone, and the one before it gets taken. The panel shows Mic off, Mic on, or Mic blocked; a lock, a logout, a lost connection, or an ended track releases the device. Tests: mic unit tests (start level, underrun, concealment, old packet, delay cap, holder, refusals), the mic channel test over the link, a real PipeWire test where pw-record records the decoded tone, the config test, three panel tests, and spikes/e2e/mic.mjs (Chromium fake microphone recorded from farglass-mic, silence before and after) pass.
