---
id: ADR-039
title: Video redirection hides the host video behind a black box
status: accepted
date: 2026-11-20
tags: []
superseded_by:
files: [extension/redirect/page.js, crates/farglass-session/src/redirect.rs]
---

## Context
ADR-038 copies the Media Source Extensions (MSE) of a page in the host browser to the client, which plays the stream under the window with a color key. The host hid the video only when the first background under it was black, and else drew it black with a CSS filter. In real use with YouTube in Brave, the agent used 22 % of one core and 470 MB. Measured with spikes/e2e/youtube.mjs (YouTube, VP9, 854 x 480) on the 2-core arm64 test host, CPU in percent of one core over 10 s:

| Host video | Host browser | Agent | Agent memory |
|---|---|---|---|
| Drawn, sent as tiles and H.264 (F037) | 11 | 55 | 68 MB |
| Drawn black with a CSS filter | 15 | 22 | 54 MB |
| Hidden, with a black box under it | 10 | 1 | 54 MB |

A filtered video still gives a new black frame for each video frame, and the agent compares each commit.

## Decision
The decision of ADR-038 stays, with one change: while the client shows a video, the host always hides it, and the extension puts a black box at the box of the video, before it in the page. The controls of the page, which come after the video, stay on top of the box. The box follows the video at each scan of 250 ms. The client sees its own video through the black, as before. The agent keeps the init segment and at most 16 MiB of the last appends of each media source for a shell that connects later, not 48 MiB.

## Consequences
A redirected video costs the agent about 1 % of a core on each page, not only on a page with a black background. The host browser still decodes the video (ADR-038). A page whose layout puts a different element between the video and its controls can show that element over the black box; such a page shows it in the host and in the client alike. A tab of the shell that reloads gets at most 16 MiB of recent video: the client can wait for the next segments before it plays.
