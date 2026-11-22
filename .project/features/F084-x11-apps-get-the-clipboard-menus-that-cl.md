---
id: F084
title: X11 apps get the clipboard, menus that close, their pointer shapes, and every key
status: done
phase:
tags: [surface, wayland]
decisions: [ADR-037]
files: [crates/farglass-surface/, crates/farglass-proto/src/message.rs, crates/farglass-session/src/surfaces.rs, web/src/surface/manager.ts, web/src/proto/generated/messages.ts, Cargo.toml, Cargo.lock, README.md, CHANGELOG.md, packaging/farglassd.conf]
---

## Goal
X11 apps behave as Wayland apps for the clipboard, menus, the pointer, and fast typing.

## Requirements
- A key right after the click that gives an X11 window the focus reaches the window: after an X11 focus change, the compositor waits for one round trip of the X11 connection
- The clipboard goes between X11 apps, Wayland apps, and the shell in each direction; the primary selection of X11 stays in X11
- The shell dismisses an X11 menu: if an X11 app holds the pointer, a thread clicks with XTEST outside each X11 window
- A pointer that a client draws itself goes to the shell as a PNG image with its hotspot (SurfaceEvent::CursorImage), at most 128 pixels on each side
- Only surfaces with the xwayland_shell role keep their first buffer; a cursor surface gives its buffer back at once
- Nobody gets the X11 display before the X11 window manager runs: a window or a clipboard of an early app reaches the window manager

## Acceptance
Tests in tests/x11.rs, 10 of 10 runs with all 7 passing: xev gets Shift_L, H, and i sent right after Activate with no wait for FocusIn, then the pointer motion at root (50,60) and button 1; xclip -i sets the X11 clipboard and the shell gets Clip::Text; SetClipboard of the shell reaches xclip -o; xterm gives a CursorImage with pixels, a hotspot in the image, and a valid PNG; a test X11 client with an override-redirect menu that grabs the pointer closes the menu after PopupDismiss, and PopupClosed follows. Unit tests of cursor::png check the straight alpha, the header, and the CRC of IEND. The trace of a failed clipboard run showed that xclip took the clipboard before the window manager started; spawn_with now returns the display only after the window manager runs. Not tested: the clipboard between a Wayland app and an X11 app, the cursor image in a browser, and Java, Wine, and SecureCRT.
