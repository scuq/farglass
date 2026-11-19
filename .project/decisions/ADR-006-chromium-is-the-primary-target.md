---
id: ADR-006
title: Chromium is the primary target
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
Keyboard Lock, Window Management, and File System Access are in Chromium only. WebCodecs and WebTransport are in Chromium, Firefox, and Safari.

## Decision
Chromium is the primary target. Firefox and Safari must work, with fewer keyboard shortcuts. The shell is a progressive web app and uses Keyboard Lock in fullscreen mode on Chromium.

## Consequences
Each feature that uses a Chromium-only API needs a fallback. Tests run on Chromium first.
