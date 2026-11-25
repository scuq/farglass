---
id: F116
title: Camera from the client to host apps
status: planned
phase: P08
tags: [camera, protocol]
decisions: [ADR-014, ADR-017, ADR-036]
files: [crates/farglass-session/src/camera.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/desktop.rs, crates/farglass-session/src/launch.rs, crates/farglass-proto/src/message.rs, web/src/camera/, web/src/panel/Panel.tsx, packaging/farglassd.conf, spikes/e2e/camera.mjs, README.md, CHANGELOG.md]
---

## Goal
A host app shows the camera of the client.

## Requirements
- Only if the spike (F115) finds a path. Its decision record gives the host node and the browser flags.
- A new channel kind, camera, from the shell to the agent. The shell sends the format, then one data frame for each encoded frame, with a key frame flag. The agent can ask for a key frame.
- The shell captures with getUserMedia and encodes with the WebCodecs VideoEncoder: H.264, 1280x720 at most, 30 frames each second at most, 1.5 Mbit/s, latency mode realtime. This uses the hardware encoder of the client.
- The agent decodes with openh264, which it has for the surfaces, and writes the frames to the camera node of the session.
- The shell captures only while a host app reads the camera node. Thus the camera light of the client is on only while a host app uses the camera.
- The camera is off after each connect. A Camera button in the panel arms it. The button shows: off, armed, in use by a host app, and refused by the browser.
- A late frame is dropped, not queued. On WebTransport the channel has its own stream (ADR-036). On WebSocket the shell drops frames while the send buffer is above a limit, so the camera does not delay input.
- One connection of a user holds the camera (ADR-017), as for the microphone (F114).
- A lock, a logout, and a lost connection stop the capture. The agent limits the frame size and the frame rate. The start and the stop go to the security log (ADR-043).
- A key camera in packaging/farglassd.conf, default true, as for the microphone. Update the README quick start and the changelog in the same edit.
- The launcher starts the host browsers with the flags of the spike.

## Acceptance
