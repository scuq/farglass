---
id: F004
title: Agent link with peer uid check
status: done
phase: P01
tags: []
decisions: [ADR-003, ADR-014, ADR-015, ADR-017]
files: [crates/farglassd/src/agents.rs, crates/farglassd/src/connection.rs, crates/farglassd/src/ws.rs, crates/farglass-session/, crates/farglass-proto/src/codec.rs, docs/protocol.md]
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
- farglassd creates the agent socket (agent_socket, default /run/farglass/agent.sock) with mode 0666 and replaces only an old socket, not a file of a different type.
- farglassd reads the uid of each agent with SO_PEERCRED. A test with the real farglass-session confirms the uid.
- farglassd relays the frames of a browser connection to the agent of its user and back. A browser connection of a different user, or with no login, does not reach the agent.
- Agents of different users connect at the same time. Two connections of one user share the agent, with separate channel numbers.
- The end of a connection closes its channels on the agent and sends detach. The end of an agent closes the channels in the browser. A new agent replaces the old one.
- farglass-session connects, does the hello exchange, refuses each channel kind, and connects again after farglassd restarts.
- 11 tests of the agent link pass in 20 runs.
