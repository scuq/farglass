---
id: F110
title: Terminal bookmarks: a folder or an ssh URL opens in a terminal
status: done
phase:
tags: [terminal, bookmarks]
decisions: []
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, crates/farglass-session/src/terminal.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/bookmarks.rs, crates/farglass-session/tests/terminal.rs, crates/farglass-session/tests/stats.rs, web/src/launcher/bookmarks.ts, web/src/wm/controller.ts, web/src/wm/Desktop.tsx, web/src/apps/terminal/, web/src/apps/bookmarks/Bookmarks.tsx, spikes/e2e/bookmarks.mjs, README.md, CHANGELOG.md]
---

## Goal
A bookmark opens a terminal window that starts ssh to a host or the shell in a folder.

## Requirements

## Acceptance
Bookmarks with browser = terminal take ssh://[user@]host[:port] or a folder (/path, ~/path, file:///path). Open::Pty has an optional PtyStart (folder | ssh); the agent runs ssh with -p PORT -- user@host as arguments and refuses a password, path, query, or a user/host that starts with - ; a missing folder refuses the channel with a message and shows as a bookmark problem. The bookmark opens a new terminal window whose first pane keeps the start, so Enter reconnects. Rust unit and integration tests, vitest 340, spikes/e2e/bookmarks.mjs 23/23 and tabs.mjs 19/19 pass.
