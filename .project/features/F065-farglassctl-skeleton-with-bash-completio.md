---
id: F065
title: farglassctl skeleton with bash completion
status: done
phase: P06
tags: [packaging]
decisions: [ADR-047]
files: [crates/farglassctl/, Cargo.toml, Cargo.lock, README.md]
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
crates/farglassctl: clap command line with install, update, uninstall, status, doctor, user (list, add, remove, disable, enable), session (list, logout), pin (set, clear), logs, version, completion. A command that changes the host refuses without root. farglassctl completion bash prints the script from clap_complete. The aarch64 musl release build is statically linked (file: statically linked; ldd: not a dynamic executable), 1.4 MB. The install of the completion file comes with F067; the subcommand bodies with F067 and F068. Tests: the command line is valid, the subcommands parse, the completion names each subcommand.
