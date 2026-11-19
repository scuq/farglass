---
id: ADR-012
title: Rust on the host, TypeScript and SolidJS in the shell, newest stable versions
status: accepted
date: 2026-11-19
tags: [toolchain, dependencies]
superseded_by:
files: [Cargo.toml, rust-toolchain.toml, web/package.json, web/.nvmrc]
---

## Context
rcd is a Rust workspace, and its login and session code can move to farglass. The shell needs updates with no tree comparison when a window moves. ADR-002 used the rcd settings, and rcd uses Rust edition 2021. farglass starts new, so it does not need old versions. The host has rustc 1.99.1 and Node.js 24.22.2.

## Decision
The host programs are one Cargo workspace. The shell uses TypeScript, SolidJS, and Vite. The terminal uses xterm.js with the WebGL renderer behind a thin interface. Use the newest stable version of each tool and library:
- Rust: rust-toolchain.toml with channel = "stable" and components rustfmt and clippy. The workspace uses edition 2024, resolver 3, and rust-version = "1.99". Raise rust-version when the code needs a newer feature.
- Rust crates: declare each crate once in [workspace.dependencies] at the newest release. Crates use dep.workspace = true.
- Node.js: the newest Active LTS release. Record it in web/.nvmrc and in engines in web/package.json.
- npm packages: the newest release of TypeScript, SolidJS, Vite, and vitest. package.json uses caret ranges. TypeScript uses strict mode and ES modules.
- Commit Cargo.lock and web/package-lock.json.
- Do not use nightly Rust, or prerelease or beta packages.

## Consequences
Code from rcd moves with small edition 2024 changes, for example unsafe extern blocks and the new rules for temporaries in if let. Smithay is available for the compositor. The build needs Rust and Node.js. Each dependency update is a separate commit that runs cargo update, npm update, and the tests. Every developer gets the same Rust version from rust-toolchain.toml.
