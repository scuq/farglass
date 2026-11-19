---
id: F002
title: Channel protocol in farglass-proto with TypeScript types
status: done
phase: P01
tags: []
decisions: [ADR-005, ADR-012, ADR-014]
files: [crates/farglass-proto/, web/src/proto/, docs/protocol.md]
---

## Goal
One definition of the frame and the channel messages for Rust and TypeScript.

## Requirements
- Frame header: channel, type, length
- Open, close, and data messages for a channel
- Generate the TypeScript types from the Rust types
- Encode and decode tests on both sides

## Acceptance
- The frame header has the channel, the type, and the length in 8 bytes. Frame and FrameHeader in farglass-proto encode and decode it, from a message and from a byte stream.
- Open, Ready, Close, and ConnectionMessage are the JSON messages. Data frames carry raw bytes.
- cargo run -p farglass-proto --example gen-ts generates web/src/proto/generated/ from the Rust types. cargo test fails when the files are old.
- 21 Rust tests and 40 TypeScript tests pass. The two sides read the same test vectors. Property tests cover the stream decode and random input.
- docs/protocol.md is the reference for the wire format.
