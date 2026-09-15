#!/bin/bash

cd /home/jim/RaspiClock || exit 1

/usr/bin/cage -- /home/jim/RaspiClock/target/release/raspiclock &
CAGE_PID=$!

# Wait briefly for Cage's Wayland output to become available.
sleep 2

WAYLAND_DISPLAY=wayland-0 \
XDG_RUNTIME_DIR=/run/user/$(id -u) \
/usr/bin/wlr-randr --output DSI-1 --transform 90

wait "$CAGE_PID"
