---
id: ADR-011
title: The tile cache is mirrored and uses slot numbers
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
ADR-007 names a tile cache by hash on the client. With a hash as the key, the host must learn what the client holds, and each reference carries a hash. The ICA protocol shows a way with no such cost.

## Decision
The host and the client run the same deterministic slot allocator in a ring. Messages name a tile by its slot number. The host finds equal tiles with its own hash table, and no hash goes to the client. This decision replaces only the cache key of ADR-007. The other parts of ADR-007 stay.

## Consequences
The host knows the client cache with no acknowledgment. The two allocators must stay equal, so a lost message breaks the cache. The surface stream must be reliable and in order, and a new connection with no resume starts with an empty cache.
