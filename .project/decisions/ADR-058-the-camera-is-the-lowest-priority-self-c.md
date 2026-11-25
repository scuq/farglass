---
id: ADR-058
title: The camera is the lowest-priority, self-clocked stream
status: accepted
date: 2026-11-25
tags: [camera, protocol]
superseded_by:
files: []
---

## Context
F116 sends the camera of the client to the host apps. The camera data shares queues with the desktop: the WebSocket of the shell, the 64-frame queue and the relay loop of farglassd, the 256-frame link queue of the user, and the one loop of the agent. The agent decoded each picture in that loop, in front of surface input and surface acks. The shell encoded on its main thread, where it handles input. A picture of 1.5 Mbit/s that waits in a queue delays each key and click behind it. Bytes in the kernel buffer of a TCP connection delay later data, and no program can change that order.

## Decision
The camera is the lowest-priority stream, and it clocks itself.

- The agent acks each picture. The shell sends a new picture only when no picture is unacked, and it skips a capture before the encode. Thus each queue holds at most one camera picture, and no reference of the stream breaks.
- The shell measures the round trip of each ack. If the median delay of 15 pictures is more than 30 ms above the minimum of 10 s, it lowers the bit rate by 25 %, down to 250 kbit/s, then the frame rate. After 5 s with no extra delay, it raises the bit rate by 10 %.
- On WebSocket, the shell sends a picture only while less than 16 KiB wait in the socket. On WebTransport, the camera stream has the lowest sendOrder.
- The shell captures and encodes in a Worker where MediaStreamTrackProcessor exists.
- The agent decodes on a thread of its own with nice 10, not in the link loop.

## Consequences
A slow uplink gets a camera with less quality, not a slow desktop. The camera frame rate goes down when the agent decodes slowly. farglassd needs no change: its queues hold one camera picture at most. If the load test shows a delay in farglassd, the next step is a byte budget for its link queue. A full bound over a shared network path is not possible: the e2e test camera-load.mjs measures the added delay of the echo channel.
