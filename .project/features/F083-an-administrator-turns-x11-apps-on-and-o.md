---
id: F083
title: An administrator turns X11 apps on and off, off by default
status: done
phase:
tags: [surface, config]
decisions: [ADR-037]
files: [crates/farglassd/src/config.rs, packaging/farglassd.conf, crates/farglass-session/src/desktop.rs, crates/farglass-session/Cargo.toml, crates/farglass-surface/tests/x11.rs, Cargo.lock, README.md, CHANGELOG.md]
---

## Goal
X11 apps are off on a new host. An administrator turns them on and off with one option, and when they are on, X11 windows cost the same as Wayland windows.

## Requirements
- x11_apps in farglassd.conf, false by default; farglassd accepts the key and does not use it
- The agent reads x11_apps from /etc/farglass/farglassd.conf (FARGLASSD_CONFIG changes the path) when it starts; a file that is not valid turns X11 apps off with a warning
- FARGLASS_XWAYLAND=1 or 0 overrides the option, for tests
- When x11_apps is true and Xwayland is missing, the journal says so
- A test shows the frame economy of an X11 window: small damage for one key, the scroll move in most frames, no frames for a window with no viewer

## Acceptance
x11_apps is false in Config::default and in packaging/farglassd.conf; the packaged-file tests of farglassd pass. The agent test x11_apps_are_off_unless_the_file_turns_them_on gives false for an empty file, a commented key, and the packaged file, true for x11_apps = true with a [tls] table, and an error for a string or a missing value; x11_wanted turns X11 off on an error. Measured with xterm on Xwayland: 0 commits/s idle and with no viewer, 56 to 59 commits/s while it scrolls with input (the 16 ms cap of Wayland windows), the frame of one key in 1.9 ms with 156 px of damage (0.05%), the scroll move in 141 of 149 frames and 83% of the rows; the test an_x11_window_gets_the_same_frame_economy_as_a_wayland_window asserts key damage under 1%, a scroll move in at least half of the frames, and no frame with no viewer, 8 of 8 runs. Not tested: a web session in a browser with x11_apps = true.
