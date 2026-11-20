---
id: F052
title: Video redirection accepts only messages of the extension
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
A web page in the host browser cannot exhaust the memory of the agent or place a video over a different window.

## Requirements
- relay.js accepts only messages of page.js, for example with a nonce that page.js gets before the scripts of the page run
- Caps in redirect.rs: medias for each browser connection, sources for each media, the size of an init segment, and the queued bytes
- The client places a video only on the window of the host browser that sent it: the exact title and the app id or pid, not a title prefix
- Review finding 4 in docs/threats.md

## Acceptance
