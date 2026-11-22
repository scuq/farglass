---
id: F088
title: The border of a Linux app window is round at the bottom corners
status: done
phase:
tags: [shell, linux-apps]
decisions: []
files: [web/src/apps/linux/linux.css, CHANGELOG.md]
---

## Goal

## Requirements

## Acceptance
The app area of a Linux window has the inner radius of the frame (5 px) at the bottom corners, 0 when maximized. The e2e popups test passes, and its screenshot shows the round amber border at both bottom corners.
