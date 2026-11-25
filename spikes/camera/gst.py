#!/usr/bin/env python3
"""Run a GStreamer pipeline, as gst-launch-1.0 does, for hosts with no gstreamer1.0-tools.

Usage: gst.py SECONDS PIPELINE...

The script stops the pipeline after SECONDS, or at the end of the stream.
It prints the number of buffers that each element named "count" received.
Exit status: 0 at the end of the stream or after SECONDS, 1 on an error.
"""

import sys
import time

import gi

gi.require_version("Gst", "1.0")
from gi.repository import GLib, Gst  # noqa: E402

Gst.init(None)
seconds = float(sys.argv[1])
pipeline = Gst.parse_launch(" ".join(sys.argv[2:]))
count = pipeline.get_by_name("count")
buffers = 0
first = None


def probe(_pad, _info):
    """Count one buffer and keep the time of the first one."""
    global buffers, first
    buffers += 1
    if first is None:
        first = time.monotonic()
    return Gst.PadProbeReturn.OK


if count is not None:
    count.get_static_pad("sink").add_probe(Gst.PadProbeType.BUFFER, probe)

loop = GLib.MainLoop()
status = 0


def message(_bus, msg):
    """Stop the loop at the end of the stream or on an error."""
    global status
    if msg.type == Gst.MessageType.EOS:
        loop.quit()
    elif msg.type == Gst.MessageType.ERROR:
        err, debug = msg.parse_error()
        print(f"error: {err.message} ({debug})", file=sys.stderr)
        status = 1
        loop.quit()


bus = pipeline.get_bus()
bus.add_signal_watch()
bus.connect("message", message)
start = time.monotonic()
pipeline.set_state(Gst.State.PLAYING)
GLib.timeout_add(int(seconds * 1000), loop.quit)
loop.run()
pipeline.set_state(Gst.State.NULL)
if count is not None:
    took = time.monotonic() - (first or start)
    print(f"buffers={buffers} seconds={took:.2f}")
sys.exit(status)
