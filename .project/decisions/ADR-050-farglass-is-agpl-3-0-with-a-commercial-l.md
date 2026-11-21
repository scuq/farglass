---
id: ADR-050
title: farglass is AGPL-3.0 with a commercial license on request
status: accepted
date: 2026-11-22
tags: [license]
superseded_by:
files: []
---

## Context
The code declared MIT OR Apache-2.0, but the repository had no LICENSE file. P06 requires a license. farglass is a server that users reach over a network. With a permissive license, a company can host a changed farglass as a closed service. The author wants to know about commercial use and to sell licenses for it.

## Decision
farglass is licensed under AGPL-3.0-only (LICENSE). The author gives a commercial license on request (COMMERCIAL.md); the contact is the GitHub repository or profile of scuq. All license fields (Cargo workspace, web and spikes/e2e package.json) say AGPL-3.0-only.

## Consequences
A contribution from another person can be merged only if its author agrees that it is available under the AGPL and under the commercial license (a CLA or a sign-off). A new dependency must have a license that is compatible with the AGPL-3.0 and also allows the commercial license, thus no GPL-only code. A later license check (for example cargo-deny) can enforce this. The shell must keep a way for users to get the source (AGPL section 13).
