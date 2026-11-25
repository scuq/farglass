---
id: F119
title: One media menu in the panel for sound, microphone, and camera
status: done
phase: P08
tags: [panel, camera]
decisions: []
files: [web/src/panel/, web/src/theme/lcars.css, spikes/e2e/lib.mjs, spikes/e2e/sound.mjs, spikes/e2e/mic.mjs, spikes/e2e/camera.mjs, spikes/e2e/camera-load.mjs, README.md, CHANGELOG.md]
---

## Goal
The panel uses one button for sound, microphone, and camera, and a live microphone or camera stays visible without a click.

## Requirements
- One panel button opens a menu with the three controls: sound, microphone, and camera. The controls keep their states and labels.
- The button shows only the exceptions: Mic on, Cam on, Cam ready, Muted. With none, it shows Media.
- A live microphone or camera highlights the button, with the text and the color: the user sees it with no click (privacy).
- The menu stays open after a toggle, so the user can change two controls.
- The states and labels come from one place, web/src/panel/media.ts.
- Update the unit tests and the e2e tests sound, mic, camera, and camera-load.

## Acceptance
The panel has one Media button for the sound, the microphone, and the camera, with a menu of three items (menuitemcheckbox) that stays open after a toggle. The labels come from web/src/panel/media.ts. With the menu closed, the button names each exception (Mic on, Cam on, Cam ready, Muted) and is highlighted while the microphone or the camera sends, so a live capture shows with no click. Tests: media summary unit tests, the panel tests (40), and the e2e tests sound, mic, camera, and camera-load (WebSocket case) pass with the menu.
