#!/usr/bin/env python3
"""animation.py - a GTK 4 app for the end-to-end tests of farglass.

The window has a label and an area of 320 x 240 pixels that changes in each
frame, as a video player does. With --tone, it also plays a sine tone of
440 Hz through GStreamer to the sound server of the session. With --small,
the area is 16 x 16 pixels: too small for a video region, as a spinner.
With --fps, it prints "fps N" each second: the frames it drew in that second.

Usage: animation.py [--tone] [--small] [--fps]
"""

import math
import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402

TONE = "--tone" in sys.argv
SMALL = "--small" in sys.argv
FPS = "--fps" in sys.argv


def start_tone():
    """Plays a sine tone until the app stops."""
    gi.require_version("Gst", "1.0")
    from gi.repository import Gst

    Gst.init(None)
    pipeline = Gst.parse_launch("audiotestsrc wave=sine freq=440 volume=0.3 ! audioconvert ! autoaudiosink")
    pipeline.set_state(Gst.State.PLAYING)
    return pipeline


def draw(area, cr, width, height, state):
    """Draws a moving pattern: smooth gradients, as in a video."""
    t = state["frame"] / 30.0
    for band in range(0, width, 8):
        phase = band / width * 2 * math.pi + t * 3
        cr.set_source_rgb(0.5 + 0.5 * math.sin(phase), 0.5 + 0.5 * math.sin(phase + 2), 0.5 + 0.5 * math.sin(phase + 4))
        cr.rectangle(band, 0, 8, height)
        cr.fill()
    cr.set_source_rgb(1, 1, 1)
    cr.arc(width / 2 + math.cos(t * 2) * width / 3, height / 2 + math.sin(t * 3) * height / 3, 30, 0, 2 * math.pi)
    cr.fill()


def activate(app):
    window = Gtk.ApplicationWindow(application=app, title="Animation")
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
    box.set_margin_top(12)
    box.set_margin_bottom(12)
    box.set_margin_start(12)
    box.set_margin_end(12)
    box.append(Gtk.Label(label="A region that changes in each frame"))
    area = Gtk.DrawingArea()
    area.set_content_width(16 if SMALL else 320)
    area.set_content_height(16 if SMALL else 240)
    state = {"frame": 0, "counted": 0}
    area.set_draw_func(draw, state)

    def tick(widget, clock):
        state["frame"] += 1
        widget.queue_draw()
        return GLib.SOURCE_CONTINUE

    area.add_tick_callback(tick)

    def report():
        print(f"fps {state['frame'] - state['counted']}", flush=True)
        state["counted"] = state["frame"]
        return GLib.SOURCE_CONTINUE

    if FPS:
        GLib.timeout_add_seconds(1, report)
    box.append(area)
    window.set_child(box)
    window.present()
    if TONE:
        app.tone = start_tone()


app = Gtk.Application(application_id="org.farglass.Animation")
app.connect("activate", activate)
app.run([sys.argv[0]])
