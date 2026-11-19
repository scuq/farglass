---
id: ADR-010
title: Product principles: no telemetry, reduced motion, measured cost
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
A desktop must be on the side of its user. The Slipstream desktop states such principles, and they cost little to adopt at the start.

## Decision
farglass sends no telemetry and does not look for updates by itself. The shell loads no resource from a third-party host. Each effect has a reduced-motion version. No feature exists to keep the attention of the user. Settings show the resource cost of the desktop.

## Consequences
Fonts, icons, and libraries ship with the shell. Each new effect needs a second version. There is no use data to guide the design.
