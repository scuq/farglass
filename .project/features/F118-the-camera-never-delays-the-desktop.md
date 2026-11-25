---
id: F118
title: The camera never delays the desktop
status: done
phase: P08
tags: [camera, performance]
decisions: [ADR-058, ADR-057]
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/tests/vectors.json, crates/farglass-session/src/camera.rs, crates/farglass-session/tests/camera.rs, web/src/camera/, web/src/proto/, web/src/panel/, spikes/e2e/camera-load.mjs, spikes/e2e/camera.mjs, spikes/e2e/package.json, spikes/camera/, docs/protocol.md, docs/threats.md, README.md, CHANGELOG.md]
---

## Goal
The camera of the client never delays the input or the pictures of the desktop, and uses less quality when the network or the host is slow.

## Requirements
- ADR-058 gives the design: an ack window of one picture, a delay-based bit rate, sendOrder on WebTransport, a 16 KiB limit on WebSocket, encode in a Worker, decode on a nice thread of the agent.
- The agent sends an ack for each picture, and closes the channel with resource-limit after more than 2 unacked pictures.
- The rate controller is a pure function with unit tests.
- spikes/e2e/camera-load.mjs measures the echo p50 and p95 with the camera off and on, on WebSocket, on WebSocket through a relay at 1 Mbit/s up, and on WebTransport. The p95 with the camera on is at most 30 ms above the p95 with the camera off.
- Update docs/protocol.md, docs/threats.md, the README, and the changelog.

## Acceptance
The camera does not delay the desktop. The agent decodes on its own thread with nice 10 and acks each picture when that thread takes it; more than 2 unacked pictures close the channel with resource-limit. The shell encodes in a Worker where MediaStreamTrackProcessor exists, sends a new picture only after the ack (one or two in flight), skips captures before the encode, sends only while less than 16 KiB wait on a WebSocket, and gives the camera the lowest WebTransport sendOrder. A rate controller takes at most half of the measured uplink and backs off on queueing delay; its simulation keeps a key press behind the camera under 40 ms from 300 kbit/s up. Tests: camera unit tests, the echo test in tests/camera.rs (each echo under 15 ms while pictures decode), rate, panel and WebTransport tests, npm --prefix spikes/e2e run camera, and npm --prefix spikes/e2e run camera-load on a release build: the camera added 0.6 ms to the echo p95 on WebTransport, 0.7 ms on WebSocket, and 7.1 ms on a WebSocket with 1 Mbit/s up, within the 30 ms bound.
