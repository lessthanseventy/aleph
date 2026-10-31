#!/usr/bin/env bash
# aleph dev on a headless Sway: a window draws the wallpaper; a save that breaks the shader is
# reported and the last good version keeps drawing; the fix reloads.
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/headless-sway.sh
wp="$run/wp"
cp -r wallpapers/rain "$wp"
fail() { echo "$1" >&2; cat "$run/aleph.log" >&2; exit 1; }

ALEPH_STATS=1 target/release/aleph dev "$wp" 2> "$run/aleph.log" &
sleep 2
[ "$(frames)" -ge 10 ] || fail "the dev window draws no frames"
good=$(cat "$wp/rain.wgsl")
echo 'fn wallpaper(p: vec2f) -> vec4f { return nope; }' > "$wp/rain.wgsl"
sleep 1
grep -q 'nope' "$run/aleph.log" || fail "a broken shader is not reported"
a=$(frames); sleep 1
[ $(( $(frames) - a )) -ge 5 ] || fail "frames stop after a broken save"
printf '%s\n' "$good" > "$wp/rain.wgsl"
sleep 1
grep -q '^aleph: reloaded rain' "$run/aleph.log" || fail "the fixed shader does not reload"
pgrep -f "aleph dev $wp" > /dev/null || fail "aleph dev exited"
echo ok
