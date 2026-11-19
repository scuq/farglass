---
id: ADR-037
title: The compositor runs on its own thread in the agent, copies each shm commit, and leaves window management to the shell
status: accepted
date: 2026-11-20
tags: [surface, wayland]
superseded_by:
files: []
---

## Context
ADR-007 puts a headless Wayland compositor on Smithay in the agent. Smithay uses calloop, the agent uses tokio, and the shell must be the only window manager. GTK 4 needs a data device and draws client-side decorations even when the compositor asks for server-side ones.

## Decision
farglass-surface runs the compositor on its own thread with a calloop loop and a stop flag, on a Unix socket that the agent selects ($XDG_RUNTIME_DIR/farglass-wayland). It offers wl_compositor, wl_subcompositor, wl_shm (ARGB8888, XRGB8888), xdg_wm_base, xdg-decoration (always server-side), wl_seat, wl_output, xdg-output, and wl_data_device_manager. Each commit with an shm buffer is copied whole and the buffer is released at once; the event carries the copy, the damage in buffer pixels (the whole buffer for a new size), and the window geometry. Frame callbacks go out at most each 16 ms. The compositor places no window: size, focus, close, and popup dismissal are commands of the shell. The agent keeps the last pixels of each surface and gives terminals WAYLAND_DISPLAY.

## Consequences
A client cannot block the compositor with a held buffer, at the cost of one copy for each commit; the tile codec (F025) reads only the damage of that copy. The shell must crop client-side shadows with the geometry. Input (F029), the clipboard (F030), the output size from the shell, and frame acknowledgment are not in this feature.
