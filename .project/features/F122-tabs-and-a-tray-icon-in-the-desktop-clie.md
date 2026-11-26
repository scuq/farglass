---
id: F122
title: Tabs and a tray icon in the desktop client
status: done
phase: P06
tags: [client]
decisions: [ADR-060]
files: [client/, README.md, CHANGELOG.md]
---

## Goal
A user has the desktops of more hosts in one window, one tab for each server, and keeps the sessions connected in the tray when the window is closed.

## Requirements
- One BrowserWindow: its page is the tab strip. The server list and each shell are WebContentsViews below the strip. The views of the shell keep the sandbox, no preload, and one session for each server (ADR-060).
- Connect opens a tab or selects the open tab of the server. A tab closes with its ×, a middle click, or a removal of the server. The beforeunload question of the shell becomes an asynchronous dialog, thus the other tabs continue.
- Ctrl+Shift+F12 server list, Ctrl+Shift+F10 and F11 the tab before and after. In full screen the active tab fills the window with no strip.
- A tray icon: a click shows or hides the window. Its menu has the tabs, the server list, Close to the tray, and Quit.
- Close to the tray is on by default, kept in servers.json, and also in the server list. With it off, a close with open tabs asks first.
- The icon comes from web/src/brand/farglass.svg through client/scripts/icon.js, for the packages and the tray.

## Acceptance
12 client tests pass, including the close to tray setting. The client from the source and the packaged arm64 client against the local farglassd: two servers open as two tabs in one window, the strip shows Servers, Local, Loopback with the selected tab marked, the window title names the active server. Close of a tab removes its view and selects the tab before. Removal of a server closes its tab. In full screen the active tab gets the full window (2560x1600) and the strip is hidden. A close of the window hides it (not destroyed) with Close to the tray on. The shell tabs have no window.farglass and no window.farglassTabs, the strip has no window.farglass. The deb has the farglass icon in hicolor 32 to 512. The tray icon and the HTML full screen of the shell were not seen on a screen. The mic test was skipped as in F121: PipeWire records no tone on this VM.
