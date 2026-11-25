#!/bin/sh
# node.sh - add a test camera node with media.class Video/Source to PipeWire (F115)
#
# Usage: node.sh SECONDS [FORMAT]
#
# The node "farglass-camera" gives a moving test picture at 1280x720 and 30
# frames each second, in FORMAT (default I420). It has media.role Camera, so
# the camera portal shows it. GStreamer stops the node when its consumer goes.
#
# Exit status: the status of gst.py.

format=${2:-I420}
exec python3 "$(dirname "$0")/gst.py" "$1" \
  videotestsrc is-live=true pattern=ball \
  ! "video/x-raw,format=$format,width=1280,height=720,framerate=30/1" \
  ! pipewiresink mode=provide client-name=farglass-camera \
  'stream-properties="props,node.name=farglass-camera,node.description=farglass-camera,media.role=Camera,media.class=Video/Source"'
