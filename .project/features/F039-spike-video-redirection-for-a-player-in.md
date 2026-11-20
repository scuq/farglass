---
id: F039
title: Spike: video redirection for a player in the host Chromium
status: done
phase: P07
tags: []
decisions: [ADR-038]
files: [spikes/e2e/redirect.mjs, spikes/e2e/youtube.mjs, extension/redirect/]
---

## Goal
Find out if the compressed stream of a web player can go to the client browser while the host does no decode.

## Requirements
- An extension in the host Chromium that takes the data that a page gives to Media Source Extensions
- The host page must continue as if it plays
- Test with one public video site
- Record the result as a decision

## Acceptance
The spike shows that the stream of a web player goes to the client: the extension copies the MSE calls of the page, and the client plays them with its own decoder in step with the host, on a local MSE page and on YouTube. The host still decodes, but the agent encodes nothing for the video: on YouTube the agent goes from 43 % to 0 % of a core and the host browser from 10 % to 9 %. The result and the limits are in ADR-038.
