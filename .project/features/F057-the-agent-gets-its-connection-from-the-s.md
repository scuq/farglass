---
id: F057
title: The agent gets its connection from the session worker
status: planned
phase: P06
tags: [security]
decisions: [ADR-041]
files: [crates/farglass-login/src/host.rs, crates/farglass-session/src/main.rs, crates/farglass-session/src/link.rs, crates/farglassd/src/agents.rs, docs/threats.md]
---

## Goal
A program of the user cannot act as the agent: it cannot connect a replacement agent, ptrace the agent, or load code into it. Then it cannot see the keys, the clipboard, or the frames that the browser and the compositor give the agent.

## Requirements
- The session worker opens the agent connection to farglassd while it is root, marks it with the uid, and gives the open socket to the agent as an inherited file descriptor
- farglassd accepts an agent for a uid only on a connection from root (SO_PEERCRED uid 0) or the farglass user, not from the uid itself
- A connection of a replaced or crashed agent is not accepted again: the session ends, and a new login starts a new agent
- The agent calls prctl(PR_SET_DUMPABLE, 0) at start, before it opens a channel
- The worker starts the agent with a fixed environment: no LD_PRELOAD, LD_LIBRARY_PATH, or other variable from the user
- A test mode for the e2e scripts that start the agent directly, refused in the installed service
- Tests: a process of the user that connects to the agent socket gets no channels; ptrace of the agent by the user fails
- docs/threats.md: the agent socket and the accepted risk of a replaced agent
- Supersede the part of ADR-041 on the replacement of the agent; the unlock through logind stays accepted

## Acceptance
