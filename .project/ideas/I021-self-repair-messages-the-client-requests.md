---
id: I021
title: Self-repair messages: the client requests a redraw of a rectangle or a window record when its state has a gap, and each message has a length so an unknown type is skipped
status: open
tags: [ica, surface]
promoted_to:
---

## Idea
The client requests a redraw of a rectangle when a message refers to a cache slot that it does not have, and requests a window record when a message names an unknown window. Requests have a rate limit. Each message has a type and a length, so a peer skips a type that it does not know. Source: the Swift ICA client at github.com/JanHaan/swift-ica-client, Thinwire/ThinwireEngine.swift and TWI/WindowModel.swift.

## Why
A defect or a lost state repairs itself with one small redraw and does not end the session. Old and new versions of the shell and the agent can work together.
