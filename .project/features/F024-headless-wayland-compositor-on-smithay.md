---
id: F024
title: Headless Wayland compositor on Smithay
status: done
phase: P05
tags: []
decisions: [ADR-037, ADR-007]
files: [crates/farglass-surface/, crates/farglass-session/src/surfaces.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/terminal.rs, crates/farglass-session/src/main.rs, crates/farglass-proto/src/message.rs]
---

## Goal
A compositor in the agent that gives each top-level window as one surface channel.

## Requirements
- xdg-shell, shm buffers, popups
- No desktop drawing. The shell is the only window manager
- Size, focus, and close come from the shell
- Buffer and damage for each commit

## Acceptance
The new crate farglass-surface is a headless Smithay 0.7 compositor on its own thread. It offers xdg-shell with popups, wl_shm buffers, and server-side xdg-decoration, and it draws nothing. Each commit gives the copied buffer, the damage in buffer pixels, and the window geometry. The shell gives size (configure), focus (activated state and keyboard focus), close, and popup dismissal as commands. The agent starts it at $XDG_RUNTIME_DIR/farglass-wayland, terminals get WAYLAND_DISPLAY, and the surfaces channel carries the windows, popups, and commit facts to the shell and the requests back. Tested by a Rust Wayland client (toplevel, shm buffer, damage clipping, configure size, activated state, close, popup and popup_done, 40 runs with no hang), surfaces hub unit tests, a link test, and a real gnome-calculator (GTK 4) that shows its window, commits 482x738 frames, and stops on the close request.
