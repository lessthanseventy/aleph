# aleph: working here

A live-wallpaper engine for Linux, headed for Steam. One Rust binary: `src/desktop.rs` (the
layer-shell front, `aleph run`), `src/headless.rs` (`aleph render`, and what the tests draw
with), `src/gpu.rs` (the one renderer both share). The design and its phases are in
`docs/plans/2026-10-30-aleph-design.md`; read it before reshaping anything.

## Law

- **Free when unseen.** Frames come from the compositor's frame callbacks and are capped at the
  wallpaper's `fps`. Never a timer loop. A change that costs idle GPU or CPU needs a measurement.
- **The GPU is the integrated one** (`gpu::adapter_options`). A wallpaper never wakes a discrete
  card.
- **One GPU device per process.** Creating Vulkan instances on several threads at once crashes
  the driver. Tests share `GPU` (`tests/render.rs`).
- **Steam runs it in sniper (glibc 2.31).** What ships is `mise run build:steam`, never a plain
  `cargo build`. A new native dependency has to load inside sniper: `mise run steam:check`.
- **No input on the live desktop.** `aleph run` and `grim` screenshots are fine; synthetic keys
  and clicks are not.

## Dev loop

`mise tasks` lists them. `mise run check` is the gate (fmt, clippy `-D warnings`, tests).
`mise run render -- NAME` draws a wallpaper headless; `mise run run -- NAME` puts it live.

## Verify

A wallpaper change is shown by a `render` PNG. A desktop change is shown by `aleph run` plus a
`grim -o OUTPUT` screenshot. A packaging change is shown by `mise run steam:check`.
