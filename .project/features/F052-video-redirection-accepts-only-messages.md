---
id: F052
title: Video redirection accepts only messages of the extension
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [extension/redirect/manifest.json, extension/redirect/page.js, extension/redirect/relay.js, crates/farglass-session/src/redirect.rs, crates/farglass-session/src/link.rs, crates/farglass-proto/src/message.rs, web/src/surface/redirect.ts, web/src/apps/linux/Linux.tsx, spikes/e2e/redirect.mjs, README.md, docs/threats.md]
---

## Goal
A web page in the host browser cannot exhaust the memory of the agent or place a video over a different window.

## Requirements
- relay.js accepts only messages of page.js, for example with a nonce that page.js gets before the scripts of the page run
- Caps in redirect.rs: medias for each browser connection, sources for each media, the size of an init segment, and the queued bytes
- The client places a video only on the window of the host browser that sent it: the exact title and the app id or pid, not a title prefix
- Review finding 4 in docs/threats.md

## Acceptance
page.js makes a MessageChannel and gives port2 to relay.js in a synchronous MessageEvent at document_start; relay.js (now first in the manifest) takes only the first event and has no window message listener. The agent caps 32 media sources per browser connection, 8 SourceBuffers per media, a 4 MiB init segment, 64 MiB kept per connection, and refuses appends to unknown SourceBuffers, with a security log line. The agent finds the host browser process (parent of the native messaging host by SO_PEERCRED) and sends its window ids in each state; the client shows a video only on such a window and only if the window title is the page title plus at most the browser name. Tests: 3 agent unit tests, vitest sameTitle; e2e redirect passes, with a new check that window.postMessage and a second port event from the page reach nothing.
