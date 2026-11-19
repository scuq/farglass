---
id: I016
title: Session resume below the cache: numbered messages and a retransmit queue, so a new connection continues with no repaint and no cache reset
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
Each message of the surface and control streams has an implicit number. The sender keeps sent messages in a retransmit queue until the peer acknowledges them, with a size limit. After a new connection, each side gives the number of the next message that it expects, and the peer sends the rest again. This layer is below the tile cache and the compressor. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, CGP.swift.

## Why
A short network loss continues with no repaint and no cache reset. Without this layer, each new connection starts with an empty cache and a full redraw of all windows.
