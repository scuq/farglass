---
id: F005
title: Echo channel in dev_user mode
status: planned
phase: P01
tags: []
decisions: [ADR-005]
files: [crates/farglassd/, crates/farglass-session/, web/src/, spikes/]
---

## Goal
A message goes from the shell to the agent and back with no login.

## Requirements
- dev_user mode skips the login and works on loopback only
- An echo channel in the agent
- An end-to-end script in spikes/ with Playwright

## Acceptance
