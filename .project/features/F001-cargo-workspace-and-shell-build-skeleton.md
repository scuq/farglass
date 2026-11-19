---
id: F001
title: Cargo workspace and shell build skeleton
status: done
phase: P01
tags: []
decisions: [ADR-012]
files: [Cargo.toml, rust-toolchain.toml, crates/, web/, README.md, .gitignore]
---

## Goal
A workspace that builds, with empty crates and an empty shell.

## Requirements
- Crates farglass-proto, farglassd, farglass-login, farglass-session
- web/ with TypeScript, SolidJS, Vite, and vitest
- Versions follow ADR-012: rust-toolchain.toml on stable, edition 2024, resolver 3, rust-version 1.99, [workspace.dependencies], web/.nvmrc, newest npm releases, both lock files committed
- cargo test --workspace and the web tests pass
- Set the test command in .project/config

## Acceptance
- The workspace has the crates farglass-proto, farglassd, farglass-login, and farglass-session. cargo test --workspace, cargo clippy, and cargo fmt --check pass.
- web/ uses TypeScript 7, SolidJS 2 with @solidjs/vite-plugin 3, Vite 8, and vitest 5 with jsdom. npm test passes. npm run build type-checks and builds.
- rust-toolchain.toml selects stable. The workspace uses edition 2024, resolver 3, and rust-version 1.99. web/.nvmrc records Node.js 26 LTS. Cargo.lock and web/package-lock.json are in the commit.
- .project/config sets the test command.
