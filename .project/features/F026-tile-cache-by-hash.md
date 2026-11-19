---
id: F026
title: Mirrored tile cache
status: done
phase: P05
tags: []
decisions: [ADR-011]
files: [crates/farglass-surface/src/cache.rs, crates/farglass-surface/src/tiles.rs, web/src/surface/decoder.ts]
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
Host and client run the same ring of 2048 slots (given in the hello message): a tile message and a keep message take the next slot on both sides, and a ref names a slot. The host finds equal tiles with an xxh3-128 hash table (tile pixels and size) and sends no hash. A hit in the oldest quarter of the ring goes as keep and ref, so tiles in use stay. Each channel starts with an empty cache. Tested by Rust tests of the ring (overwrite of the oldest, keep), an exact round trip with the reference decoder on a full ring, vitest tests of ref and keep, and measured hit rates: 99 % on a synthetic text page (Rust test) and 44 % on the gnome-calculator toolbar and buttons in spikes/e2e/surface.mjs (90 KiB instead of 108 KiB).
