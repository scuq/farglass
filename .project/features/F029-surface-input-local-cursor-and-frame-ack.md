---
id: F029
title: Surface input, local cursor, and frame acknowledgment
status: done
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/src/state.rs, crates/farglass-session/src/surfaces.rs, crates/farglass-session/src/surface_stream.rs, crates/farglass-session/src/link.rs, crates/farglass-proto/src/message.rs, web/src/surface/, web/src/apps/linux/Linux.tsx, spikes/e2e/surface.mjs]
---

## Goal
Input has no delay other than the network, and a slow network gets the newest state only.

## Requirements
- Keyboard and pointer events as Wayland seat events
- The client draws the cursor from a shape message
- Input on its own stream
- The client acknowledges frames, and the host drops old state

## Acceptance
Keyboard and pointer input go on a surface-input channel (its own stream with WebTransport): pointer motion with the surface id and logical coordinates, DOM buttons mapped to BTN codes, wheel as axis, and keys as Linux key codes from KeyboardEvent.code. The compositor turns them into wl_pointer and wl_keyboard events (keys go to the activated window, xkb codes plus 8). The compositor offers cursor-shape-v1 and sends the cursor of the app as a CSS name, which the shell sets on the window. A press outside open popups closes them. The client acknowledges each frame; the agent sends at most 2 frames without an acknowledgment, and waiting commits merge so a slow network gets the newest state only. Tested by a Rust Wayland client (enter, motion, button, axis, key 30 to the focused window), an agent test (6 commits with no ack give 2 frames, then one frame with the newest pixels after the ack), and spikes/e2e/surface.mjs with gnome-calculator: 7 + 2 Enter typed in Chromium computes 9, a click on C clears it, the cursor is text over the entry field, and 306 frames encode at p50 98 us.
