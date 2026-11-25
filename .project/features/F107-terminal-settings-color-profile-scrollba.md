---
id: F107
title: Terminal settings: color profile, scrollback, and font
status: done
phase:
tags: [terminal, settings]
decisions: []
files: [web/src/apps/terminal/, web/src/apps/settings/, CHANGELOG.md]
---

## Goal
Users choose the color profile, the font, the font size, and the scrollback of the terminal in Settings > Terminal.

## Requirements

## Acceptance
Settings > Terminal has cards for color profile (theme plus six presets with 16 ANSI colors), font and size with a sample, and scrollback. The value is the state-store key terminal; each open terminal applies a change at once through TerminalEngine.configure. tsc and vitest pass.
