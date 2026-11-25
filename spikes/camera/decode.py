#!/usr/bin/env python3
"""Measure the CPU cost of an H.264 decode of 720p at 30 frames each second with openh264 (F115).

Usage: decode.py [SECONDS]

The script encodes SECONDS (default 10) of two test pictures with the openh264
encoder of GStreamer at 2.5 Mbit/s: "moving", a SMPTE picture that moves, and
"noise", random pixels, the worst case. It then decodes each file with
openh264dec as fast as possible and prints the CPU time of the decode as a
part of one core at 30 frames each second.

Exit status: 0 if all pipelines succeed, 1 if not.
"""

import os
import resource
import subprocess
import sys
import tempfile

seconds = int(sys.argv[1]) if len(sys.argv) > 1 else 10
frames = seconds * 30
gst = os.path.join(os.path.dirname(os.path.abspath(__file__)), "gst.py")
pictures = {
    "moving": "videotestsrc pattern=smpte horizontal-speed=4",
    "noise": "videotestsrc pattern=snow",
}
caps = "video/x-raw,format=I420,width=1280,height=720,framerate=30/1"


def run(pipeline):
    """Run a pipeline and return the CPU seconds of the process."""
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    subprocess.run([sys.executable, gst, "600", pipeline], check=True, stdout=subprocess.DEVNULL)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    return (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)


with tempfile.TemporaryDirectory() as tmp:
    for name, source in pictures.items():
        path = os.path.join(tmp, f"{name}.mp4")
        run(f"{source} num-buffers={frames} ! {caps} ! openh264enc bitrate=2500000 rate-control=bitrate "
            f"! h264parse ! mp4mux ! filesink location={path}")
        idle = run(f"filesrc location={path} ! qtdemux ! h264parse ! fakesink sync=false")
        busy = run(f"filesrc location={path} ! qtdemux ! h264parse ! openh264dec ! fakesink sync=false")
        decode = busy - idle
        size = os.path.getsize(path) * 8 / seconds / 1e6
        print(f"{name}: {size:.2f} Mbit/s, decode {decode:.2f} s CPU for {seconds} s of video,"
              f" {100 * decode / seconds:.1f} % of one core")
