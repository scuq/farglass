---
id: F057
title: The agent gets its connection from the session worker
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043, ADR-044]
files: [crates/farglass-login/src/broker.rs, crates/farglass-login/src/worker.rs, crates/farglass-session/src/broker.rs, crates/farglass-session/src/main.rs, crates/farglass-session/src/link.rs, crates/farglassd/src/agents.rs, crates/farglassd/src/config.rs, crates/farglassd/src/server.rs, crates/farglassd/src/main.rs, crates/farglass-proto/src/message.rs, README.md, docs/threats.md]
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
The session worker (root) makes a socket pair, gives one end to the agent in FARGLASS_BROKER_FD, and answers each request with a link: it connects to the agent socket, sends LinkMessage::Claim{uid}, and passes the link with SCM_RIGHTS. farglassd reads the claim only from a root peer (read_claim reads exactly one frame of at most 256 bytes, uid not 0) and refuses an agent of the user unless --allow-user-agents or dev_user, with a security log line. The agent takes the broker before other threads, sets close-on-exec, calls prctl(PR_SET_DUMPABLE, 0), and asks the worker again after a lost link. ADR-044 supersedes ADR-041 without the replacement risk. Tests: farglassd agents.rs broker round trip, claim parser, refusal of a user agent; farglass-session protect.rs (environ and mem of the agent are closed to the user); e2e login, pin, passkey, browsers, apps pass. Not tested: the worker as root in a real web session (sudo spikes/session/check.sh).
