---
id: ADR-017
title: One agent link for each user carries all browser connections of the user
status: accepted
date: 2026-11-19
tags: [agent, protocol]
superseded_by:
files: [crates/farglassd/src/agents.rs, crates/farglassd/src/connection.rs, crates/farglass-session/]
---

## Context
A user can have more than one browser connection, for example two tabs (ADR-004). Many users work at the same time (ADR-015). Each shell numbers its channels from 1. The agent connects to farglassd, not the opposite (ADR-003).

## Decision
- Each user has one agent and one agent link. A new agent of a user replaces the old one.
- farglassd gives each channel of a browser connection a new odd number on the agent link. It keeps the routes, frees a number after the close frames of the two sides, and rewrites only the frame header.
- Channel 0 of the agent link carries LinkMessage: hello, attach, and detach. The shell never sees LinkMessage.
- When a browser connection stops, farglassd closes its open channels on the agent with cancelled and sends detach. When the agent stops, the browser gets internal-error for each open channel.
- The connection logic in connection.rs uses a stream and a sink of frames. It does not know the transport.
- The numbers that a side opens increase. farglassd refuses a lower or a used number.
- In version 1, the agent cannot open channels. An agent that tries it loses its link.

## Consequences
Agent code sees one link and one channel space, with attach and detach for the state of each connection. WebTransport (F023) can use connection.rs with no change. A slow browser connection can delay the other connections of the same user, because they share the agent link. Flow control (F014) must fix this. Channels that the agent opens, for example notifications (F020), need a target connection in a later protocol version.
