---
id: ADR-014
title: Frame format and channel rules of protocol version 1
status: accepted
date: 2026-11-19
tags: [protocol]
superseded_by:
files: [crates/farglass-proto/, web/src/proto/, docs/protocol.md]
---

## Context
ADR-005 gives the frame as a header (channel, type, length) and a payload, and says that the protocol must not depend on one ordered stream. farglassd reads frames from the network before login. The shell and the host can open channels at the same time. Rust and TypeScript each have a codec, and the two can drift apart.

## Decision
docs/protocol.md is the reference. The main rules:
- The header has 8 bytes, little-endian: channel u32, type u8, length u24.
- The largest payload is 1 MiB. A receiver refuses a larger length when it reads the header.
- The frame types are open, ready, data, control, and close. All frames of a channel go on that channel. No central control channel opens or closes other channels.
- Channel 0 is the connection channel. It accepts only control frames. Each side sends a hello message with the protocol version first.
- The shell opens odd channel numbers, and the host opens even numbers. A connection uses each number one time only.
- The opener can send data before it gets the ready frame.
- A receiver ignores an unknown JSON field and refuses an unknown variant.
- The Rust types are the one definition. An example program generates the TypeScript files, and cargo test fails when they are old. The two test suites read the same test vectors in crates/farglass-proto/tests/vectors.json.

## Consequences
A channel can move to its own QUIC stream with no change to its frames (F023). Each new channel kind adds a variant to Open and a row to docs/protocol.md. Each protocol change adds a test vector. A sender must divide data of more than 1 MiB. Flow control (F014) and keepalive (F010) need new messages. A change that breaks an old peer increases PROTOCOL_VERSION.
