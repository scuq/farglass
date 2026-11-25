---
id: ADR-056
title: The camera of a session is a PipeWire node, and the agent shows it to the camera portal
status: accepted
date: 2026-11-25
tags: [camera, pipewire]
superseded_by:
files: []
---

## Context
F115 tested how a host app can get the camera of the client with no kernel module. The test host was Debian 13 on arm64 with PipeWire 1.4.2, WirePlumber 0.5.8, xdg-desktop-portal 1.20.3, Chromium 159, and Firefox ESR 153. The scripts are in spikes/camera/.

- A PipeWire stream node with media.class Video/Source and media.role Camera works. GStreamer pipewiresrc reads it at 30 frames each second.
- Chromium and Firefox open a PipeWire camera only through the camera portal. The portal gives the app a PipeWire connection, hides all nodes on it, and leaves the cameras to the session manager.
- WirePlumber shows the cameras only to a connection with pipewire.access=portal. PipeWire marks the connection of the portal as unrestricted. Thus the app sees no camera, also not a real camera of the host, and Chromium uses V4L2.
- When a client of the user gives that connection the permissions rwx on the node, Chromium shows the node as its only camera. It gets 1280x720 at 30 frames each second in I420, NV12, and YUY2. Firefox gets 27 frames each second in headless mode.
- Chromium needs --enable-features=WebRtcPipeWireCamera, and Firefox needs the pref media.webrtc.camera.allow-pipewire=true. Without them, both use V4L2.
- The GTK backend of farglass-portals.conf serves Lockdown and Access. If org.gnome.desktop.privacy disable-camera is true, the portal refuses all cameras.
- The first camera access of a host app opens the Access dialog of the portal. The permission store keeps the answer in the table devices, id camera, app id "".
- With no portal backend, the portal has no Camera interface.
- EnumFormat can give the size and the frame rate as plain values or as a choice of the type None. Both work.
- openh264 decodes H.264 at 720p and 30 frames each second with 2.4 % of one core at 3.9 Mbit/s, and with 16 % for noise at 60 Mbit/s. The host was a VM with two arm64 cores. amd64 is not measured.
- In the client Chromium, a page with a self-signed certificate (ADR-016) is a secure context after the user clicks Proceed. getUserMedia gives a camera and a microphone track, and VideoEncoder exists.
- v4l2loopback (path B) needs a kernel module through DKMS, root to load it, and a signed module when Secure Boot is on. Path A works, thus path B is not necessary.

## Decision
Path A. The agent adds the PipeWire stream node farglass-camera with media.class Video/Source and media.role Camera.

- The node is the driver of its graph. A timer starts each frame.
- EnumFormat gives I420 at the size of the client and the frame rate of the client, as plain values.
- The agent watches the PipeWire clients of the user. It gives rwx on farglass-camera, and on no other node, to each client with pipewire.access.portal.app_id "" and the media role Camera. This is the work that WirePlumber does not do on Debian 13.
- The launcher starts Chromium with --enable-features=WebRtcPipeWireCamera.
- Firefox gets no change from farglass. A user who wants the camera in Firefox sets media.webrtc.camera.allow-pipewire to true in about:config.
- The agent does not change the permission store or the lockdown setting of the user.

## Consequences
F116 can start. It needs no kernel module and no root.

Host apps in the web session see the camera of the client and not the cameras of the host. A native app that reads PipeWire directly, for example GStreamer or OBS, also sees the node. An app that reads only V4L2 devices does not see it.

The PipeWire server is the one of the user. Thus apps of a local session of the same user also see farglass-camera while the agent runs.

A system policy for the Firefox pref is not an option. It changes Firefox for local sessions too, and there no agent gives the permissions, so Firefox finds no camera.

The first host app that opens the camera shows the Access dialog of the portal in the shell. If disable-camera is true, no host app gets the camera, and F116 must tell the user why.

If a later WirePlumber or PipeWire shows the cameras to portal connections, the grant of the agent changes nothing. Then examine again if the agent must hide the cameras of the host.

amd64 decode cost: F116 measures it.
