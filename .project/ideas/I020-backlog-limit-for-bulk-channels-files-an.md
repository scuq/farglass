---
id: I020
title: Backlog limit for bulk channels: files and clipboard wait while surface or input data is not acknowledged
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
Bulk channels, for example file transfer and large clipboard data, wait while the amount of data that is not acknowledged is above a limit. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, WD/ICASession.swift, with a limit of 2 MiB.

## Why
Surface data and input never wait behind a large copy. With WebTransport, stream priority does part of this work, and the limit covers the WebSocket transport.
