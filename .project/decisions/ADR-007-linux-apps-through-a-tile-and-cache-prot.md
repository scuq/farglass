---
id: ADR-007
title: Linux apps through a tile and cache protocol, as RDP and ICA do
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
A video codec for each window costs too much CPU on a host with no GPU and makes text less sharp. RDP and ICA are fast on such hosts because they do not send the screen as one video.

## Decision
farglass-session runs a headless Wayland compositor built on Smithay. Each top-level window is one surface channel. The pipeline uses these techniques: damage from the Wayland clients, lossless tiles for text and interface, lossy tiles with a later lossless pass for images, a tile cache by hash on the client, scroll detection with move commands, a cursor that the client draws, input on its own stream, and frame acknowledgment. A video codec is only for a region that changes all the time. Video redirection sends the compressed stream of a player to the client browser. Try each technique that RDP and ICA use. Video is the last slice.

## Consequences
An idle window costs no CPU. Text is exact. The project owns a compositor and a codec set. Until the video slice, video in a window is slow.
