---
id: F004
title: Agent link with peer uid check
status: planned
phase: P01
tags: []
decisions: [ADR-003, ADR-015]
files: [crates/farglassd/src/agents.rs, crates/farglass-session/]
---

## Goal
farglass-session connects to farglassd, and farglassd knows its user.

## Requirements
- Unix socket /run/farglass/agent.sock
- Read the peer uid with SO_PEERCRED
- Start from rcd/crates/rcdd/src/agents.rs
- farglassd relays frames between the browser connection and the agent
- Agents of different users connect at the same time. farglassd relays each browser connection only to the agent of its user (ADR-015)

## Acceptance
