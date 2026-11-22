---
id: F085
title: The frame of a Linux app window follows the size of the app
status: done
phase:
tags: [surface, wm]
decisions: []
files: [web/src/apps/linux/Linux.tsx, web/src/apps/linux/fit.ts, web/src/apps/linux/fit.test.ts, crates/farglass-surface/src/state.rs, crates/farglass-surface/src/state/x11.rs, crates/farglass-surface/tests/x11.rs, README.md, CHANGELOG.md]
---

## Goal
After a resize, the window frame has the size of the app, also when the app answers with a size of its own.

## Requirements
- 300 ms after the last new size of the app, the frame takes that size if it differs by more than 1 px from the content
- A size that the shell asked for and the app did not answer yet stops this for 2 s at most
- A snapped or maximized window keeps its size
- The compositor clamps the size of an X11 window to the smallest and the largest size of WM_NORMAL_HINTS

## Acceptance
A user reported that a smaller SecureCRT window kept the size of the drag around a smaller app. With xterm, the compositor gives each size up and down (900x600, 300x200, 700x500, 250x150): the fault is in the shell, which compared the app size only at the first new frame after a resize. fit.test.ts has 7 tests of the decision: keep, fit after a snap or an own change, wait for an open request, no wait after 2 s or after the answer. The X11 test an_x11_window_keeps_the_smallest_and_the_largest_size_of_the_app gives 300x200 for 100x80 and 800x600 for 1000x900 with min 300x200 and max 800x600; all 8 X11 tests pass in 3 of 3 runs. Not tested: SecureCRT itself and a resize in a browser.
