---
id: P08
title: M7 microphone and camera
status: planned
---

## Goal
A host app can use the microphone and the camera of the client, for example in a video conference.

## Exit criteria
A video conference in the host Chromium hears the microphone of the client and shows its camera. Each one is off until the user turns it on, and the panel shows when it is on. If the spike finds no camera path with no kernel module, the phase records why and ends with the microphone only.
