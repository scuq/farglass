---
id: F018
title: dbus channel
status: done
phase: P04
tags: []
decisions: [ADR-005, ADR-009, ADR-031]
files: [crates/farglass-session/src/dbus.rs, crates/farglass-session/src/link.rs, crates/farglass-proto/src/message.rs, web/src/dbus.ts, web/src/apps/about/, docs/protocol.md, Cargo.toml]
---

## Goal
The shell calls methods, reads properties, and gets signals on the session bus and the system bus.

## Requirements
- An allowlist of bus names in the agent
- JSON form of D-Bus values
- Signal subscriptions that end with the channel

## Acceptance
- The agent has an allowlist of bus names for each bus. A call to org.freedesktop.systemd1 and a subscription to it give org.freedesktop.DBus.Error.AccessDenied.
- JSON form of D-Bus values in two directions: unit tests for each basic type, arrays, dictionaries with number keys, structures, and variants. A test service on the session bus echoed an a{sv} with nested variants. The error of a service comes back with its D-Bus name, for example NameHasNoOwner.
- Signal subscriptions: a test signal came to the shell. After the unsubscribe and after the end of the channel, no signal came.
- The system bus works: hostname1 gave its Hostname property as a variant. In Chromium, About shows the operating system of the host from hostname1.
- The TypeScript client has call, getProperty, getAllProperties, setProperty, and subscribe, with 5 tests.
