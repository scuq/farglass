---
id: ADR-038
title: Video redirection through an extension of the host browser and a color key
status: accepted
date: 2026-11-20
tags: []
superseded_by:
files: [extension/redirect/, crates/farglass-session/src/redirect.rs, web/src/surface/redirect.ts]
---

## Context
The spike F039 asked if the compressed stream of a web player can go to the client browser while the host does no decode. A page gives the stream to the browser through Media Source Extensions (MSE). Measured on the 2-core arm64 test host, with spikes/e2e/redirect.mjs (VP8, 640 x 360) and spikes/e2e/youtube.mjs (YouTube, VP9, 854 x 480), CPU in percent of one core over 10 s:

| Host video | Host browser (test page) | Agent (test page) | Host browser (YouTube) | Agent (YouTube) |
|---|---|---|---|---|
| Drawn, sent as tiles and H.264 (F037) | 5 | 38 | 10 | 43 |
| Drawn black with a CSS filter | 9 | 18 | 15 | 23 |
| Hidden with visibility | 3 | 0 | 9 | 0 |

Chromium decodes a hidden video too: the host decode does not stop. A black filter still gives a new black frame for each video frame, thus the agent still diffs commits. A hidden video gives no frames.

## Decision
An extension in the host browser copies the MSE calls of each page and the state of each video element. A script in the world of the page wraps addSourceBuffer, appendBuffer, remove, abort, the duration, endOfStream, and URL.createObjectURL. A relay in the isolated world encodes each append in base64 for the service worker. The service worker gives the messages to the native messaging host org.farglass.redirect: farglass-session with the origin of the extension as its argument. It copies them to the socket farglass-redirect.sock of the agent. The agent gives them to the redirect channel of the shell, and keeps the init segment and the last appends for a shell that connects later.

The client makes a MediaSource with the same SourceBuffers and appends the same data, in order, to a muted video element under the canvas of the window. The sound stays on the sound path (F038). The client follows the time, the pause, and the rate of the host every 250 ms, and seeks when it is more than 0.4 s away.

While the client shows a video, the host hides it if the first background under it is black, as in most players, and else draws it black with a filter. The worker of the window makes the pure black pixels of the rectangle of the video transparent: a color key, as RDP and ICA overlays do. The controls of the page stay on top of the client video.

The launcher gives --load-extension to Chromium and Brave. The agent writes the native messaging manifest for Chromium, Brave, and Google Chrome in a web session.

## Consequences
A video in the host browser costs the agent nothing to encode, and the client shows the stream of the site at its own quality with its own decoder. The host still decodes: the spike result is partial. A page that stops to decode with no visible video would need a fake media element, and that breaks players.

The client gets the resolution that the player of the host selects for its size. A pure black pixel of the controls shows the video under it. Media Source Extensions in a worker (MediaSourceHandle) and encrypted media (EME) are not copied or do not play on the client: such a video stays as tiles. Google Chrome ignores --load-extension: it needs a policy install, a later step. A page can send false messages to the native messaging host, but they change only the video of its own window, and each message has at most 64 MiB.
