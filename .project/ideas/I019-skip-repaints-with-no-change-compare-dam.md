---
id: I019
title: Skip repaints with no change: compare damaged tiles with the last sent state before the host encodes them
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
Before the host encodes a damaged tile, it compares the tile with the state that the client has. An equal tile is not sent. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Thinwire/Framebuffer.swift, which compares rows before it shows a frame.

## Why
Wayland apps often report more damage than changed, for example a full window for a cursor blink. The comparison costs less than the encode and the network data.
