---
id: F112
title: Network check shows the own IP address from ~/.outgoingip
status: done
phase:
tags: [network]
decisions: [ADR-055]
files: [crates/farglass-proto/src/message.rs, web/src/proto/generated/messages.ts, crates/farglass-session/src/netcheck.rs, web/src/apps/netcheck/, README.md, CHANGELOG.md]
---

## Goal
The Network check app shows the own IP address of the host at the top, from the file ~/.outgoingip.

## Requirements

## Acceptance
NetcheckEvent::Loaded has an optional outgoing_ip: the first line of ~/.outgoingip, trimmed, at most 100 characters, read at each load (thus Check again reads it again). No file, a file that cannot be read, or an empty first line gives none. The summary at the top shows Own IP address first when it is set, and nothing with no file. Tests: a unit test for no file, a value, and an empty file; a vitest test for no address and an address.
