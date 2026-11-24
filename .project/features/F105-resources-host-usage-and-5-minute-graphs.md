---
id: F105
title: Resources: host usage and 5-minute graphs
status: done
phase: P03
tags: [settings, stats]
decisions: []
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, crates/farglass-session/src/stats.rs, crates/farglass-session/tests/stats.rs, web/src/stats.ts, web/src/shell.css, web/src/apps/settings/, spikes/e2e/settings.mjs, README.md, CHANGELOG.md]
---

## Goal
The resource page shows the use of the whole host, and a graph of the last 5 minutes for each number of the host and of farglass.

## Requirements
- The sample of the agent has an optional host: CPU in percent of all cores, memory in use and total, network in and out of the hardware devices, from /proc
- An old shell ignores the field, and a new shell with an old agent shows no card for the host
- The stats client keeps 150 samples, with the data rate of the tab from the bytes between two samples
- A graph is inline SVG with CSS variables, with no library: it follows each theme and accent color
- A graph with two series has a dashed second line and a key; the pointer shows the value at a time
- The page tells when the agent sent no sample for 6 s, and shows the numbers dim
- No graph in the list of apps

## Acceptance
The sample of the agent has an optional host with the CPU in percent of all cores, the memory in use and the total, and the bytes each second in and out of the network devices with hardware, from /proc/stat, /proc/meminfo, and /proc/net/dev. The stats client keeps 150 samples with the data rate of the tab. Settings > Resources has the card This host with CPU, memory, and network, and the card Cost of the desktop with farglassd, the agent, and the connection of the tab. Each number has a graph in inline SVG with the accent color, the time that it shows, and the peak. The pointer over a graph shows the value at that time. A graph with two series has a dashed second line and a key. With no sample for 6 s, the page tells that the numbers are old and shows them dim. A sample with no host gives no card for the host. The list of apps has no graph. Tests: stats.rs unit tests check the three parsers and the use over an interval, tests/stats.rs gets a host from a real agent, sparkline.test.ts checks the paths, the scale, and the readout, resources.test.tsx checks the numbers, the line after two samples, the key, and the old numbers. settings.mjs passed in Chromium with a real agent: the CPU of the host, six graphs with a line in the accent color, and the value below the pointer. A screenshot after 72 s under load showed the graphs. Not tested: a full history of 5 minutes in a browser, and the graphs in the light theme.
