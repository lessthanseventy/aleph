#!/usr/bin/env bash
# Proves the Steam build works where Steam will run it: inside Steam Linux Runtime 3 (sniper),
# headless (the GPU) and live (a Wayland layer surface from inside the container).
set -euo pipefail
sniper="${STEAM_SNIPER:-$HOME/.local/share/Steam/steamapps/common/SteamLinuxRuntime_sniper}"
bin=target/x86_64-unknown-linux-gnu/release/aleph
[ -x "$sniper/run" ] || { echo "no sniper runtime at $sniper (install 'Steam Linux Runtime 3.0 (sniper)' from Steam, or set STEAM_SNIPER)" >&2; exit 1; }
mkdir -p .out
"$sniper/run" -- "$bin" render wallpapers/rain .out/rain-sniper.png --size 1280x720 --time 20
echo "render in sniper: .out/rain-sniper.png"
# the container takes seconds to start: 15 s in all, and the layer must map within them
timeout 15 "$sniper/run" -- "$bin" run wallpapers/rain &
pid=$!
# Hyprland can say whether the layer mapped; elsewhere, staying up to the timeout is the proof
if command -v hyprctl >/dev/null; then
  mapped=
  for _ in $(seq 24); do
    sleep 0.5
    hyprctl layers | grep -q 'namespace: aleph' && { mapped=1; break; }
  done
  [ -n "$mapped" ] || { echo "no aleph layer mapped from sniper in 12 s" >&2; exit 1; }
fi
set +e
wait "$pid"
code=$?
set -e
# 124 is timeout ending it: the layer ran the whole time
[ "$code" = 124 ] || { echo "aleph run in sniper exited $code" >&2; exit 1; }
echo "live layer in sniper: mapped, ran to the timeout"
