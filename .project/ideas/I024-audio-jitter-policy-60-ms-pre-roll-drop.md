---
id: I024
title: Audio jitter policy: 60 ms pre-roll, drop the oldest sound above a lag limit, an empty packet for a lost frame
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
The player waits for 60 ms of sound before it starts. Above a lag limit of 0.6 s it drops the oldest sound. An empty packet marks a lost frame, and the player fills it with silence. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Audio/AudioPlayer.swift.

## Why
These numbers come from a client that works. They are a start point for the sound feature in the video and sound slice.
