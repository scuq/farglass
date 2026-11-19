---
id: F026
title: Mirrored tile cache
status: planned
phase: P05
tags: []
decisions: [ADR-007, ADR-011]
files: [crates/farglass-surface/src/cache.rs, web/src/surface/]
---

## Goal
The host does not send a tile that the client has, and knows the client cache with no acknowledgment.

## Requirements
- The host and the client run the same deterministic slot allocator in a ring. The host gives the ring size at the start
- A message that adds a tile takes the next slot on both sides. A reference names the slot
- A keep message moves tiles that are still in use to new slots
- The host finds equal tiles with its own hash table. No hash goes to the client
- A new connection with no resume starts with an empty cache on both sides
- Measure the hit rate on a text page and on a toolbar

## Acceptance
