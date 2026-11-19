---
id: F021
title: Lock screen
status: done
phase: P04
tags: []
decisions: [ADR-034]
files: [crates/farglassd/src/lock.rs, crates/farglassd/src/presence.rs, crates/farglassd/src/connection.rs, crates/farglassd/src/login.rs, crates/farglassd/src/http.rs, crates/farglassd/src/server.rs, crates/farglass-session/src/logind.rs, crates/farglass-proto/src/message.rs, web/src/App.tsx, web/src/LoginScreen.tsx, web/src/wm/activity.ts, web/src/panel/Panel.tsx]
---

## Goal
A locked session opens only after a PAM login.

## Requirements
- farglassd blocks all channels while the session is locked
- Lock from the session menu and after an idle time
- Follow the lock and unlock signals of logind

## Acceptance
Lock in the session menu, 15 minutes with no keyboard or pointer activity (idle_lock_minutes), and the Lock signal of logind lock the session. farglassd then closes each connection of the user, and a new connection gets only the lock screen: farglassd refuses each agent channel. Only a PAM login of the same user (the username is fixed) unlocks it, and each other tab on the lock screen connects again with the desktop. The agent sets LockedHint and relays Lock and Unlock of logind. Tested by Rust tests (presence, lock and unlock through login, idle lock, logind requests), vitest tests, and spikes/e2e/login.mjs (lock in one tab, wrong password, reload, unlock in a different tab).
