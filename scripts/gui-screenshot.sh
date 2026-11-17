#!/usr/bin/env bash
# Runs the GUI on a virtual X display, replays xdotool commands and saves a screenshot.
# Needs: Xvfb, xdotool, ImageMagick (import), libxkbcommon-x11.
#
#   scripts/gui-screenshot.sh out.png [worksheet.xmath|""] ['xdotool ...' ...]
#
# The first command should usually focus the window (there is no window manager):
#   'W=$(xdotool search --name xmath | head -1); xdotool windowfocus $W'
# Each command runs after a 0.7 s pause. Use `import -window root step.png`
# as a command to capture intermediate states. App log: /tmp/xmath-gui.log
set -u
OUT=$1; FILE=${2:-}
shift 2 || shift $#
DISP=:$((90 + RANDOM % 9))
Xvfb "$DISP" -screen 0 1280x880x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
export DISPLAY=$DISP WAYLAND_DISPLAY= LIBGL_ALWAYS_SOFTWARE=1
BIN=${XMATH_BIN:-./target/debug/xmath}
if [ -n "$FILE" ]; then "$BIN" "$FILE" >/tmp/xmath-gui.log 2>&1 & else "$BIN" >/tmp/xmath-gui.log 2>&1 & fi
APID=$!
sleep 4
for c in "$@"; do eval "$c"; sleep 0.7; done
import -window root "$OUT"
kill "$APID" "$XPID" 2>/dev/null
wait 2>/dev/null
grep -m3 -i panicked /tmp/xmath-gui.log && exit 1
exit 0
