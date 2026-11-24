---
id: F100
title: Bookmarks app with browser, profile, and private window
status: done
phase: P04
tags: [shell, apps]
decisions: []
files: [crates/farglass-session/src/bookmarks.rs, crates/farglass-session/src/browsers.rs, crates/farglass-session/src/launch.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/lib.rs, crates/farglass-session/Cargo.toml, crates/farglass-session/tests/audio.rs, Cargo.toml, Cargo.lock, crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, web/src/apps/bookmarks/, web/src/launcher/bookmarks.ts, web/src/links.ts, web/src/links.test.ts, web/src/panel/Launcher.tsx, web/src/panel/Panel.test.tsx, web/src/wm/Desktop.tsx, web/src/apps/apps.test.tsx, web/src/menu/menu.test.tsx, CHANGELOG.md, README.md]
---

## Goal
The user keeps URLs with short names in one file that can be copied, and opens each one in a chosen host browser, profile, or private window, or in a tab of the client browser.

## Requirements
- The file is $XDG_CONFIG_HOME/farglass/bookmarks.toml: [options] launcher, and [[bookmark]] name, url, browser, profile, private
- Browsers: firefox, chromium, chrome, brave, client, or a desktop id; profile is the name that the browser shows
- The app adds, edits, and deletes bookmarks; a save keeps the comments of the file; a change by hand shows in the app
- Open takes only the name; the agent reads the URL from the file and accepts http and https only
- Firefox gets -P NAME --new-tab or --private-window; the Chromium family gets --profile-directory=DIR and --incognito
- client opens a tab of the client browser with the label that the link leaves the host
- Bookmarks are launcher results only when options.launcher is true; the default is false

## Acceptance
bookmarks.rs and browsers.rs tests cover the parse of the file, the save that keeps comments and order, the refusal to save over a broken file, http and https only, the Firefox profiles.ini and Chromium Local State profiles, and the argv for each family with and without private mode. exec_args_with puts the URL at the place of %u. Panel.test.tsx: the launcher shows no bookmark by default and shows them with the option; Enter sends open for a host bookmark and opens a client tab with noopener for a client bookmark; the app shows the problem of a bookmark and saves a new one with its profile. The audio test skips when PipeWire gives the sink no packet. Not done yet: a manual run with real Firefox and Chromium profiles on a host.
