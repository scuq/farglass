---
id: F042
title: Launcher as explorer
status: planned
phase: P03
tags: [shell]
decisions: [ADR-001]
files: [web/src/launcher/]
---

## Goal
One search field finds apps, settings pages, and files, and calculates expressions.

## Requirements
- Results come while you type, with the keyboard only
- Settings pages are results
- Calculations and unit conversions run in the client browser
- File results come from the fs channel when it exists

## Acceptance
