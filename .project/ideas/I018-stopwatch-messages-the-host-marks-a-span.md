---
id: I018
title: Stopwatch messages: the host marks a span of its stream and the client reports the decode and draw time
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
The host puts a start mark and a stop mark with an id in its stream. The client measures the time between the two marks and sends it back. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Thinwire/ThinwireEngine.swift, opcodes 0xAE and 0xAF.

## Why
The host learns how long the client needs to decode and draw. It can then lower the frame rate or change the codec for a slow client, and the numbers serve the speed tests.
