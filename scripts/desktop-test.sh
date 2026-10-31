#!/usr/bin/env bash
# aleph on a headless Sway, off the real screen. Counts frames drawn over 3 s on a bare desktop,
# under a tiled window, under a fullscreen one, and after the window closes. Fails unless frames
# flow when visible, stop under the fullscreen window, and aleph's own pause (foreign-toplevel)
# fires and lifts: Sway also stops a covered layer's frame callbacks, other compositors may not.
set -euo pipefail
cd "$(dirname "$0")/.."
wp="${1:-wallpapers/rain}"
. scripts/headless-sway.sh

ALEPH_STATS=1 target/release/aleph run "$wp" 2> "$run/aleph.log" &
count() { local a; a=$(frames); sleep 3; echo $(( $(frames) - a )); }

sleep 2
bare=$(count)
"$foot" sleep 60 > "$run/foot.log" 2>&1 &
win=$!
sleep 1
tiled=$(count)
msg fullscreen enable
sleep 0.5
fullscreen=$(count)
grep -q '^aleph: paused' "$run/aleph.log" || { echo "aleph did not pause under the fullscreen window" >&2; exit 1; }
msg fullscreen disable
sleep 0.5
grep -q '^aleph: resumed' "$run/aleph.log" || { echo "aleph did not resume when fullscreen ended" >&2; exit 1; }
kill "$win"
sleep 0.5
back=$(count)
echo "frames in 3 s: bare=$bare tiled=$tiled fullscreen=$fullscreen back=$back"
grep -i error "$run/aleph.log" && exit 1
[ "$bare" -ge 20 ] && [ "$back" -ge 20 ] || { echo "frames do not flow on a visible desktop" >&2; exit 1; }
[ "$fullscreen" -le 2 ] || { echo "frames still drawn under a fullscreen window" >&2; exit 1; }
echo ok
