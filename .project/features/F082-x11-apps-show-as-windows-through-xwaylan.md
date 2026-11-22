---
id: F082
title: X11 apps show as windows through XWayland
status: done
phase:
tags: [surface, wayland]
decisions: [ADR-037]
files: [crates/farglass-surface/, crates/farglass-session/src/desktop.rs, crates/farglass-session/src/launch.rs, crates/farglass-session/src/terminal.rs, crates/farglass-session/src/surfaces.rs, crates/farglass-session/src/main.rs, crates/farglassctl/src/os.rs, Cargo.toml, Cargo.lock, README.md, CHANGELOG.md]
---

## Goal
An app that has only an X11 backend, for example a Java Swing app, Wine, or SecureCRT, shows as a window of the desktop.

## Requirements
- The compositor starts Xwayland, if it is installed, and is its X11 window manager (Smithay feature xwayland)
- Each X11 top-level window is one surface with the same events and commands as a Wayland window: title, class as app id, parent, size, focus, keyboard, pointer, close
- An override-redirect window, for example a menu or a tooltip, shows as a popup at its place relative to its window
- Terminals and apps of the launcher get DISPLAY. An app with a Wayland backend continues to use Wayland
- FARGLASS_XWAYLAND=0 in the environment of the agent turns X11 off, for tests; on a host, the removal of the Xwayland package turns it off
- farglassctl installs the Xwayland package of each distribution family
- Not in this feature: the clipboard of X11 apps, the dismissal of an X11 menu by a click outside it, and the pointer shapes of X11 apps

## Acceptance
The compositor starts Xwayland with Compositor::spawn_with(socket, true, ...) and is its window manager through Smithay xwayland. The test x11 starts a real xclock on DISPLAY of the compositor: ToplevelNew comes with the pid of xclock, ToplevelChanged with the title xclock and the app id XClock, a commit with pixels, a commit with 300x200 after Configure, and ToplevelClosed after Close, and xclock stops. The keyboard focus is a Wayland surface or an X11 window, and the Wayland tests pass unchanged. An override-redirect window becomes PopupNew relative to its window. The agent gives DISPLAY to terminals, launcher apps, and the user manager when Xwayland started, and QT_QPA_PLATFORM=wayland;xcb. farglassctl installs xwayland (Debian family) and xorg-x11-server-Xwayland (RHEL family). Not tested: a web session in a browser, keys into an X11 window, an X11 menu, Java, Wine, and SecureCRT.
