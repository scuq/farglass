---
id: F074
title: Smart is the default video mode
status: done
phase: P07
tags: [surface, video]
decisions: []
files: [web/src/surface/redirect.ts, web/src/surface/redirect.test.ts, web/src/apps/settings/pages/Appearance.tsx, README.md, spikes/e2e/redirect.mjs]
---

## Goal
Smart (F073) is the default video mode: an unset or unknown setting gives smart. The user found that it works well in real use, and accepts the agent CPU while the pointer is on a video.

## Requirements

## Acceptance
parseVideoMode gives smart for an unset or unknown value, and the RedirectManager and the settings page start in smart; Settings > Appearance shows Smart as the default. A stored client or host choice stays. The web tests pass, and spikes/e2e/redirect.mjs passes all steps: it sets the client mode for steps 1 to 5 and tests smart in step 6. The README tells the three modes and the cost of smart.
