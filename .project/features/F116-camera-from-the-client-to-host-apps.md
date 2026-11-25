---
id: F116
title: Camera from the client to host apps
status: done
phase: P08
tags: [camera, protocol]
decisions: [ADR-014, ADR-017, ADR-036]
files: [crates/farglass-session/src/camera.rs, crates/farglass-session/tests/camera.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/lib.rs, crates/farglass-session/src/main.rs, crates/farglass-session/src/desktop.rs, crates/farglass-session/src/launch.rs, crates/farglass-session/Cargo.toml, crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, crates/farglass-proto/tests/vectors.json, crates/farglassd/src/config.rs, web/src/camera/, web/src/proto/, web/src/panel/, web/src/wm/Desktop.tsx, web/src/theme/lcars.css, packaging/farglassd.conf, spikes/e2e/camera.mjs, spikes/e2e/package.json, docs/protocol.md, docs/threats.md, README.md, CHANGELOG.md]
---

## Goal
A host app shows the camera of the client.

## Requirements
- The path, the node, and the browser flags are in ADR-056 (spike F115).
- A new channel kind, camera, from the shell to the agent. The shell sends the format, then one data frame for each encoded frame, with a key frame flag. The agent can ask for a key frame.
- The shell captures with getUserMedia and encodes with the WebCodecs VideoEncoder: H.264, 1280x720 at most, 30 frames each second at most, 1.5 Mbit/s, latency mode realtime. This uses the hardware encoder of the client.
- The agent decodes with openh264, which it has for the surfaces, and writes I420 frames to the node farglass-camera (ADR-056). The node is the driver of its graph.
- The agent gives rwx on farglass-camera, and on no other node, to each PipeWire client with pipewire.access.portal.app_id "" and the media role Camera.
- The shell captures only while a host app reads the camera node. Thus the camera light of the client is on only while a host app uses the camera.
- The camera is off after each connect. A Camera button in the panel arms it. The button shows: off, armed, in use by a host app, and refused by the browser.
- If org.gnome.desktop.privacy disable-camera is true, the agent tells the shell, and the panel shows that the host setting blocks the camera.
- A late frame is dropped, not queued. On WebTransport the channel has its own stream (ADR-036). On WebSocket the shell drops frames while the send buffer is above a limit, so the camera does not delay input.
- One connection of a user holds the camera (ADR-017), as for the microphone (F114).
- A lock, a logout, and a lost connection stop the capture. The agent limits the frame size and the frame rate. The start and the stop go to the security log (ADR-043).
- A key camera in packaging/farglassd.conf, default true, as for the microphone. Update the README quick start and the changelog in the same edit.
- The launcher starts Chromium with --enable-features=WebRtcPipeWireCamera. The README tells Firefox users to set media.webrtc.camera.allow-pipewire.
- The e2e test answers the Access dialog of the portal, or uses a private portal as spikes/camera/portal.sh does.
- The amd64 decode cost moves to F117: the host of this work is arm64.

## Acceptance
A host app shows the camera of the client. The agent adds the PipeWire node farglass-camera (I420 1280x720 at 30 pictures each second, ADR-057) and gives only that node to the camera-portal connections of host apps (ADR-056). The shell arms the camera from the panel (Cam off, Cam ready, Cam on, Cam blocked, Cam disabled) and captures only while a host app reads the node. It sends H.264 from the WebCodecs VideoEncoder and drops late pictures. The agent decodes with openh264, limits size and rate, logs arm, start, stop, and disarm, and honors camera = false and the privacy switch of the host. The launcher adds --enable-features=WebRtcPipeWireCamera to Chromium. Tests: camera unit tests, the channel and real PipeWire tests in tests/camera.rs, the panel and WebTransport tests, and npm --prefix spikes/e2e run camera: the fake camera of the client Chromium reaches a host Chromium through the camera portal as its only camera at 1280x720 and 30 pictures each second. The amd64 decode cost moves to F117.
