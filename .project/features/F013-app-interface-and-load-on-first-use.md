---
id: F013
title: App interface and load on first use
status: planned
phase: P03
tags: []
decisions: [ADR-001]
files: [web/src/apps/]
---

## Goal
One small interface for all apps: open a window, open a channel.

## Requirements
- Each app is an ES module that loads on first use
- An app registers its name, icon, and start function
- The shell restores app windows after it connects again

## Acceptance
