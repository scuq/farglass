---
id: F115
title: Spike: a camera device on the host with no kernel module
status: done
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
Path A works with no kernel module (ADR-056). A PipeWire node farglass-camera with media.class Video/Source and media.role Camera reaches the host Chromium 159 (with --enable-features=WebRtcPipeWireCamera) and Firefox ESR 153 (with media.webrtc.camera.allow-pipewire) at 1280x720 and 27 to 30 frames each second, after the agent gives the portal connection of host apps rwx on the node. Without that grant, Debian 13 shows no PipeWire camera to host apps. A GStreamer pipewiresrc reads the node directly. openh264 decodes 720p30 with 2.4 % to 16 % of one arm64 core; amd64 is left to F116. getUserMedia works in the client Chromium on a self-signed certificate after Proceed. v4l2loopback is not necessary. The scripts are in spikes/camera/.
