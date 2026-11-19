---
id: ADR-002
title: Rust on the host, TypeScript and SolidJS in the shell
status: superseded
date: 2026-11-19
tags: []
superseded_by: ADR-012
files: []
---

## Context
rcd is a Rust workspace, and its login and session code can move to farglass. The shell needs updates with no tree comparison when a window moves.

## Decision
The host programs are one Cargo workspace with the rcd settings. The shell uses TypeScript, SolidJS, and Vite. The terminal uses xterm.js with the WebGL renderer behind a thin interface.

## Consequences
Code from rcd moves with small changes. Smithay is available for the compositor. The build needs Rust and Node.js.
