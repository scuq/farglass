---
id: F115
title: Spike: a camera device on the host with no kernel module
status: planned
phase: P08
tags: [camera, spike]
decisions: [ADR-008]
files: [spikes/camera/]
---

## Goal
Know how the agent gives the camera of the client to host apps, with no kernel module if that is possible.

## Requirements
- Path A: the agent adds a PipeWire node with media.class Video/Source. Test if the host Chromium (ADR-008) shows it as a camera in the web session: with the flag WebRtcPipeWireCamera, with xdg-desktop-portal and its camera portal, and with no portal backend of a desktop.
- Test the same node in Firefox and in one native app, for example a GStreamer pipewiresrc.
- Path B: v4l2loopback. Record what it costs: a kernel module, DKMS, root, and Secure Boot. Use it only if path A fails.
- Measure the decode of H.264 720p at 30 frames each second with openh264 on the host: the CPU load of one core on amd64 and on arm64.
- Test if getUserMedia works in the client Chromium with the self-signed certificate (ADR-016) after the user accepts the warning.
- Result: a decision record with the path, the pixel format of the node, and the launch flags of the browsers. If no path works, the record says why and F116 is dropped.
- The code of the spike stays in spikes/ and does not go into the crates.

## Acceptance
