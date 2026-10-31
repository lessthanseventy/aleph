#!/usr/bin/env bash
# aleph on a headless Sway, off the real screen. Counts frames drawn over 3 s on a bare desktop,
# under a tiled window, under a fullscreen one, and after the window closes. Fails unless frames
# flow when visible, stop under the fullscreen window, and aleph's own pause (foreign-toplevel)
# fires and lifts: Sway also stops a covered layer's frame callbacks, other compositors may not.
set -euo pipefail
cd "$(dirname "$0")/.."
wp="${1:-wallpapers/rain}"
sway=$(nix build --no-link --print-out-paths nixpkgs#sway)/bin/sway
foot=$(nix build --no-link --print-out-paths nixpkgs#foot)/bin/foot
mkdir -p .out
run=$(mktemp -d -p "$PWD/.out" desktop-XXXXXX)
chmod 700 "$run"
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT
printf 'output HEADLESS-1 resolution 1280x720\ndefault_border none\n' > "$run/config"
env -u WAYLAND_DISPLAY -u DISPLAY XDG_RUNTIME_DIR="$run" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
  "$sway" -c "$run/config" > "$run/sway.log" 2>&1 &
for _ in $(seq 50); do [ -S "$run/wayland-1" ] && break; sleep 0.1; done
export XDG_RUNTIME_DIR="$run" WAYLAND_DISPLAY=wayland-1
SWAYSOCK=$(ls "$run"/sway-ipc.*.sock)
export SWAYSOCK
msg() { "$(dirname "$sway")/swaymsg" "$@" > /dev/null; }

ALEPH_STATS=1 target/release/aleph run "$wp" 2> "$run/aleph.log" &
frames() { grep -c '^aleph: frame' "$run/aleph.log" || true; }
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
