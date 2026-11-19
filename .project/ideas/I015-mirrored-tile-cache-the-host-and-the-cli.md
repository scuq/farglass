---
id: I015
title: Mirrored tile cache: the host and the client run the same slot allocator, so the host knows the client cache with no acknowledgment
status: promoted
tags: [ica, surface]
promoted_to: F026
---

## Idea
The host and the client run the same deterministic allocator for cache slots: the next slot in order, in a ring of a size that the host gives at the start. A message that adds a tile takes the next slot on both sides. Later messages name a tile by its slot number. The ring overwrites old tiles. A keep message moves tiles that are still in use to new slots. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Thinwire/TWCache.swift.

## Why
The host knows the content of the client cache with no acknowledgment and sends no hash. A reference to a tile is two bytes. The host can still find equal tiles with a hash table of its own.
