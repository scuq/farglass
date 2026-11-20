---
id: F046
title: Reduced motion and a resource score in settings
status: done
phase: P04
tags: [shell]
decisions: [ADR-010]
files: [web/src/appearance.ts, web/src/apps/settings/pages/Appearance.tsx, web/src/apps/settings/pages/Resources.tsx, web/src/proto/connection.ts, web/src/notify/notify.css, web/src/panel/panel.css, web/src/apps/terminal/xterm.ts]
---

## Goal
Each effect has a reduced-motion version, and settings show what the desktop costs.

## Requirements
- Follow prefers-reduced-motion of the client by default, with a setting to change it
- The change applies with no reload
- The resource score shows the CPU and memory of farglassd and the agent, and the data rate of the connection

## Acceptance
Motion follows prefers-reduced-motion of the client by default, with Full and Reduced in Settings > Appearance; data-motion on the root element changes at once, also when the client setting changes, with no reload; each effect has a reduced version (the notification fades with no movement, the busy dot does not pulse, the terminal cursor does not blink). Settings > Resources shows CPU and memory of farglassd (through the peer of the agent socket) and the agent, the data rate of the connection of the tab, and a score of light, moderate, or heavy. Tests: appearance motion, resource score and page tests pass; a real run showed farglassd 25 MB and the agent 24 MB with the score light.
