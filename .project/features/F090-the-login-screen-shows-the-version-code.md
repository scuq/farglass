---
id: F090
title: The login screen shows the version code FG-MMmmpp
status: done
phase:
tags: [shell, brand]
decisions: []
files: [web/src/brand/LcarsFrame.tsx, web/src/brand/version.ts, web/src/brand/version.test.ts, web/vite.config.ts, web/src/vite-env.d.ts, CHANGELOG.md]
---

## Goal

## Requirements

## Acceptance
The LCARS frame of the login, lock, and PIN screens shows FG- and two digits per version part from web/package.json, FG-000104 for 0.1.4. Unit tests cover the format and that the build takes the version of package.json.
