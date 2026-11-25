---
id: ADR-057
title: The camera node has one fixed format, and the shell scales to it
status: accepted
date: 2026-11-25
tags: [camera, pipewire]
superseded_by:
files: []
---

## Context
ADR-056 gives the results of the spike F115 and the path: a PipeWire node that the agent shows to the camera portal. It says that the node gives the size and the frame rate of the client. But the node must exist before a host app opens it, and the shell opens the camera of the client only while a host app reads the node. Thus the agent does not know the size of the client when it adds the node. A change of the format of a node that a host app reads needs a new negotiation with each app.

## Decision
All of ADR-056 stays, with one change: the node farglass-camera has one fixed format, I420 at 1280x720 and 30 frames each second, as plain values in EnumFormat.

- The agent sends this format in the ready message of the camera channel.
- The shell crops each picture of the camera to 16:9 and scales it to the format with the VideoEncoder.
- The agent refuses a stream of a different size.
- While no stream comes, the node gives black pictures.

## Consequences
A host app always sees one format, also when the user arms the camera after the app opened it. A camera of 4:3 loses a part of its top and bottom. A client camera with less than 720 lines gets a scaled picture. A later change to more formats needs a node that the agent adds again, or a new negotiation.
