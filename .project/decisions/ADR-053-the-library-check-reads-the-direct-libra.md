---
id: ADR-053
title: The library check reads the direct libraries
status: accepted
date: 2026-11-22
tags: []
superseded_by:
files: [packaging/release.sh]
---

## Context
ADR-048 lists the dynamic libraries that a host program can link, and packaging/release.sh tested the list with ldd. ldd also shows the libraries that libpam of the build container loads. On Rocky Linux 9, libpam loads libeconf, and the first release build failed for farglass-login. These libraries are different on each distribution, and the pam package of each host installs them.

## Decision
The list of ADR-048 stays. packaging/release.sh tests only the NEEDED entries of each program (readelf -d), not the output of ldd. A library that a listed library loads belongs to the package of that library on the host. The smoke test of the release workflow continues to run ldd on each distribution and fails on a library that is not found.

## Consequences
A new direct library needs a change of the list in packaging/release.sh and a new decision. A different libpam, libpipewire, or libxkbcommon of a distribution cannot fail the build. ADR-048 is superseded only in its sentence on the check.
