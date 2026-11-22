---
id: ADR-048
title: The dynamic libraries of the host programs
status: superseded
date: 2026-11-21
tags: [packaging]
superseded_by: ADR-053
files: []
---

## Context
ADR-047 planned libgcc and libstdc++ static in the host programs. A test build shows that libstdc++ goes static with CXXSTDLIB=static=stdc++, but the Rust standard library links libgcc_s itself for unwinding: -static-libgcc does not remove it. libpam brings libaudit and libcap-ng.

## Decision
As ADR-047, with this list of the dynamic libraries that farglassd, farglass-login, and farglass-session can link: libc, libm, libgcc_s (in the base system of each distribution with glibc), libpam with libaudit and libcap-ng, libpipewire-0.3, and libxkbcommon, with the dynamic loader and the vDSO. libstdc++ is static. packaging/release.sh fails a build that links a different library. farglassctl stays fully static.

## Consequences
A new dynamic library needs a change of this decision and of the list in packaging/release.sh. farglassctl installs the packages of libpam, libpipewire, and libxkbcommon; libc and libgcc_s are in each base system.
