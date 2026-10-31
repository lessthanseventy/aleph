# aleph

Live wallpapers for Linux. Shaders behind your desktop on every monitor, free when you can't see
them, and headed for Steam with a Workshop.

> "…the Aleph, the only place on earth where all places are seen from every angle." (Borges)

Early: one wallpaper kind (WGSL shaders) on wlr-layer-shell compositors (Hyprland, Sway, KDE,
niri, COSMIC). The plan and its phases are in
[`docs/plans/2026-10-30-aleph-design.md`](docs/plans/2026-10-30-aleph-design.md).

```sh
mise run run -- rain       # rain behind every output, until Ctrl-C
mise run render -- rain    # one frame to .out/rain.png
```

## A wallpaper

A wallpaper is a folder:

```toml
# wallpaper.toml
name = "rain"
kind = "shader"
entry = "rain.wgsl"
fps = 15          # the rate it changes at; aleph never draws faster
```

```wgsl
// rain.wgsl: px is the pixel centre, origin top left; return sRGB
fn wallpaper(px: vec2f) -> vec4f {
    return vec4f(px / aleph.res, 0.5 + 0.5 * sin(aleph.time), 1.0);
}
```

`aleph.res` (pixels), `aleph.time` (seconds) and `aleph.seed` (different on each output) are in
scope. See `src/preamble.wgsl`.
