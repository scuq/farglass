---
id: F065
title: farglassctl skeleton with bash completion
status: planned
phase: P06
tags: [packaging]
decisions: [ADR-046]
files: []
---

## Goal
A static farglassctl binary with the subcommands of the administrator and an embedded bash completion.

## Requirements
- Crate crates/farglassctl, built static for x86_64 and aarch64 musl
- Subcommands: install, update, uninstall, status, doctor, user (list, add, remove, disable, enable), session (list, logout), pin (set, clear), logs, version, completion
- farglassctl completion bash prints the completion script; install writes it to /usr/share/bash-completion/completions/farglassctl
- Each command that changes the host needs root and says so
- Tests: the command line and the completion script

## Acceptance
