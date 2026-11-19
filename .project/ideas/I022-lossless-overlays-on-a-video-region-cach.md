---
id: I022
title: Lossless overlays on a video region: cached exact patches for text that the client draws on each decoded picture
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
A region with video uses a video codec. The host sends exact patches for text and sharp edges in that region as cached tiles, and the client draws them on each decoded picture until the host removes them. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Thinwire/ThinwireEngine.swift, opcodes 0xC6 and 0xC7.

## Why
Text on or near video stays sharp with no second codec decision for each tile. This is a candidate for the video slice.
