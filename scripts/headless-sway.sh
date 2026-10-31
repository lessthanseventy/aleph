# Sourced: a headless Sway in a fresh run dir under .out, killed on exit. Sets $run, $sway, $foot,
# exports the Wayland and Sway sockets, and defines msg (swaymsg) and frames (frames drawn so far).
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
frames() { grep -c '^aleph: frame' "$run/aleph.log" || true; }
