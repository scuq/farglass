---
id: F073
title: Smart video mode: the host draws the video while the pointer is on it
status: planned
phase: P07
tags: [surface, video]
decisions: [ADR-038, ADR-039]
files: [crates/farglass-session/src/redirect.rs, crates/farglass-surface/src/video.rs, extension/redirect/page.js, web/src/surface/redirect.ts, web/src/surface/worker.ts, web/src/apps/settings/pages/Appearance.tsx, spikes/e2e/redirect.mjs]
---

## Goal
A third video mode, Smart: the client plays the video, and the host draws it only while the user uses the controls of the player. The user does not see the change.

## Requirements
- Settings > Appearance > Videos in the host browser has a third choice, Smart
- The client video plays and follows the host in all modes; the sound stays on the host path
- When the pointer comes on the rectangle of the video, the client asks the host to show the video; the host frames cover the client video with no gap
- After 3 s with no pointer movement on the video while it plays, or when the pointer leaves, the host hides the video again
- In Smart mode, the client follows the host time with small rate changes, so a change of side shows no jump
- The agent gets the rectangle of the video before the first frame, so the video codec starts at once, at a higher quality for this short time
- Measure: the delay to the first host frame, the agent CPU with the pointer on and off the video, and the time difference at the change
- A paused video with the pointer on it costs the agent no encode
- After the measurement, decide if Smart becomes the default

## Acceptance
