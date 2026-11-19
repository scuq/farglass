---
id: I017
title: Adaptive frame rate: full rate on input or window change, low rate after 2 s with no input, through the frame callbacks of the compositor
status: promoted
tags: [ica, surface]
promoted_to: F048
---

## Idea
The compositor sends frame callbacks to the Wayland apps at the full rate after input or a window change, and at a low rate, for example 5 frames each second, after 2 s with no input. A region with video keeps its rate. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, ICAViewer/SessionController.swift, which sends a frame rate limit of 30 or 5 to the host.

## Why
An app that draws all the time costs host CPU for encode and for its own work. A host with no GPU has no spare CPU. The user sees no difference, because input brings the full rate back immediately.
