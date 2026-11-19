---
id: F022
title: Settings app
status: done
phase: P04
tags: []
decisions: [ADR-035, ADR-009]
files: [web/src/apps/settings/, web/src/appearance.ts, web/src/shell.css, web/src/apps/types.ts, web/src/apps/AppHost.tsx, web/src/state.ts, web/src/wm/controller.ts, web/src/apps/terminal/xterm.ts, spikes/e2e/settings.mjs]
---

## Goal
Settings pages on the dbus channel.

## Requirements
- Appearance, from the farglass configuration
- Date and time, hostname and system information
- Network, from NetworkManager
- Users, sessions, and power, from login1

## Acceptance
The settings app has six pages on the dbus channel. Appearance (theme, accent, text size) is a key of the session in the state store and applies at once in each tab, also to the terminal. Date and time (timedate1: timezone with a check against ListTimezones, network time), System (hostname1: hostname with an RFC 1123 check, display name, facts), Network (NetworkManager devices, states, addresses, read-only), Users and sessions (login1, End with a confirm step that marks this session), Power (Restart and Power off after a confirm step, CanReboot and CanPowerOff). Changes use polkit. A reload now saves a pending desktop change at once. Tested by 11 vitest tests and spikes/e2e/settings.mjs on the real system bus of this host.
