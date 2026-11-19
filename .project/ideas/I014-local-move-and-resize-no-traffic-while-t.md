---
id: I014
title: Local move and resize: no traffic while the user drags, a snapshot shows in the window, one size message at the end, live content after a frame at the new size or a timeout
status: promoted
tags: [ica, surface]
promoted_to: F047
---

## Idea
While the user moves or changes the size of a surface window, the shell sends nothing. The window shows a snapshot of its last frame. The snapshot stays at the top left, and the shell extends its edge pixels into new area. At the end of the gesture the shell sends one size message. Live content shows again when a frame at the new size arrives, or after 500 ms. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, TWI/TWILocalGesture.swift.

## Why
A drag feels immediate at each network delay. The host app draws one new size, not one for each pointer event, which saves host CPU.
