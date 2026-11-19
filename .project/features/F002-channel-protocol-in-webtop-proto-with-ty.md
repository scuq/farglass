---
id: F002
title: Channel protocol in farglass-proto with TypeScript types
status: planned
phase: P01
tags: []
decisions: [ADR-005]
files: [crates/farglass-proto/, web/src/proto/]
---

## Goal
One definition of the frame and the channel messages for Rust and TypeScript.

## Requirements
- Frame header: channel, type, length
- Open, close, and data messages for a channel
- Generate the TypeScript types from the Rust types
- Encode and decode tests on both sides

## Acceptance
