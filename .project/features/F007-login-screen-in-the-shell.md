---
id: F007
title: Login screen in the shell
status: planned
phase: P02
tags: []
decisions: [ADR-004, ADR-015]
files: [web/src/login/]
---

## Goal
The shell shows each PAM prompt and sends the answer.

## Requirements
- Prompts with and without echo, information and error messages
- A new prompt for a one-time code after the password
- A clear message when the rate limiter blocks the login
- A confirm step when the user has a local graphical session: the session will end, and work that is not saved will be lost (ADR-015)

## Acceptance
