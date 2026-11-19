---
id: F005
title: Echo channel in dev_user mode
status: done
phase: P01
tags: []
decisions: [ADR-005]
files: [crates/farglassd/, crates/farglass-session/, web/src/, spikes/e2e/, packaging/farglassd.conf]
---

## Goal
A message goes from the shell to the agent and back with no login.

## Requirements
- dev_user mode skips the login and works on loopback only
- An echo channel in the agent
- An end-to-end script in spikes/ with Playwright

## Acceptance
- dev_user (option, --dev-user, FARGLASSD_DEV_USER) gives each browser connection a user with no login. farglassd refuses the option unless listen is a loopback address, and refuses connections from other addresses. Tests confirm the two checks.
- farglass-session has an echo channel: ready, each data frame and control frame back, and a close answer. An unknown kind gets not-supported.
- The shell has a channel client (web/src/proto/connection.ts) with tests, and an echo panel that shows the round-trip time.
- spikes/e2e/echo.mjs starts farglassd and the agent and uses Chromium through Playwright. Two tabs each get only their own echo. The round trip on loopback with a debug build was 12 ms.
