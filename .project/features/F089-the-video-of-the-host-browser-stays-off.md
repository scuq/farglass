---
id: F089
title: The video of the host browser stays off the window border
status: done
phase:
tags: [shell, redirect]
decisions: []
files: [web/src/apps/linux/linux.css, spikes/e2e/border.mjs, spikes/e2e/package.json, CHANGELOG.md]
---

## Goal

## Requirements

## Acceptance
The container of the client videos clips 1px off the window border (clip-path inset). The new e2e border test (372 samples over focus changes, an overlapping window, and the smart-mode handover) and the redirect e2e test pass. The overlay case of a GPU client browser is not reproducible headless.
