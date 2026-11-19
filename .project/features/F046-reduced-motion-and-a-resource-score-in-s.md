---
id: F046
title: Reduced motion and a resource score in settings
status: planned
phase: P04
tags: [shell]
decisions: [ADR-010]
files: [web/src/apps/settings/, web/src/]
---

## Goal
Each effect has a reduced-motion version, and settings show what the desktop costs.

## Requirements
- Follow prefers-reduced-motion of the client by default, with a setting to change it
- The change applies with no reload
- The resource score shows the CPU and memory of farglassd and the agent, and the data rate of the connection

## Acceptance
