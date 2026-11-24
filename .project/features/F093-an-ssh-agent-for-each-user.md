---
id: F093
title: An ssh-agent for each user
status: done
phase:
tags: [session, ssh]
decisions: []
files: [crates/farglass-session/src/ssh_agent.rs, crates/farglass-session/src/desktop.rs, crates/farglass-session/src/main.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/lib.rs, crates/farglassd/src/config.rs, packaging/farglassd.conf, spikes/e2e/term.mjs, README.md, CHANGELOG.md]
---

## Goal
Each user of farglass has one ssh-agent for the web session, so ssh and git in the terminals and apps of the desktop can use loaded keys. The keys can go when the session locks.

## Requirements
- farglass-session starts ssh-agent -D on a socket next to the Wayland socket, through setpriv --pdeathsig KILL if installed, and stops it with the session
- Terminals, apps, the user manager, and D-Bus activation get SSH_AUTH_SOCK
- A farglassd.conf key ssh_agent (default true) turns the agent off; FARGLASS_SSH_AGENT=0/1 overrides it for tests
- A farglassd.conf key ssh_agent_clear_on_lock (default true) removes the keys with ssh-add -D when farglassd locks the session

## Acceptance
farglass-session starts ssh-agent on a socket next to the Wayland socket and gives SSH_AUTH_SOCK to terminals, apps, and the user manager. The ssh-agent stops with the session, also after SIGKILL of the agent (tested: no ssh-agent left after the e2e). ssh_agent and ssh_agent_clear_on_lock in farglassd.conf, on by default. Unit tests cover the config, a key in the agent, and the clear; the terminal e2e checks SSH_AUTH_SOCK and ssh-add -l in the shell. The clear on a real lock is not tested end to end.
