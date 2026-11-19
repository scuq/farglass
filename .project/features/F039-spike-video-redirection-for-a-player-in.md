---
id: F039
title: Spike: video redirection for a player in the host Chromium
status: planned
phase: P07
tags: []
decisions: [ADR-007, ADR-008]
files: [spikes/]
---

## Goal
Find out if the compressed stream of a web player can go to the client browser while the host does no decode.

## Requirements
- An extension in the host Chromium that takes the data that a page gives to Media Source Extensions
- The host page must continue as if it plays
- Test with one public video site
- Record the result as a decision

## Acceptance
