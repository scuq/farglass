---
id: I023
title: Lossless tile codec with copy-from-row-above runs, a small color cache, and delta values, as RLE v2 in Thinwire
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
A lossless tile codec with these parts: runs that copy from the row above, runs of one color, a small color cache with one-byte hits, and delta values against the last pixel. A counter selects the cheaper literal mode with no flag in the stream. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Thinwire/TWBitmap.swift, RLE v2.

## Why
Interface content has few colors and many equal rows, so this codec is very cheap to encode on a CPU. Measure it against zstd and LZ4 on real windows before the selection.
