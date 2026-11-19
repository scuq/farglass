---
id: P01
title: M0 skeleton: workspace, web server, echo channel
status: done
---

## Goal
Prove the path from the shell in the client browser to the agent on the host, with no login.

## Exit criteria
The shell loads from farglassd through TLS. A message on the echo channel goes from the shell to farglass-session and back in dev_user mode on loopback.
