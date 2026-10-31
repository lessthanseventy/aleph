# aleph: a live-wallpaper engine for Linux, sold on Steam

*2026-10-30. Status: design + slice 1. The working name is aleph. A name check against the Steam
store comes before the store page.*

> "…the Aleph, the only place on earth where all places are seen from every angle."
> (Borges, *The Aleph*)

## 1. What it is

aleph is a Linux-native app that runs animated wallpapers behind your desktop, on every monitor,
and costs nothing while you can't see them. It is sold on Steam, and Steam Workshop is where the
wallpapers come from. Wallpaper Engine proved the category on Windows. aleph is its own product for
Linux, with its own format, its own name and its own wallpapers. It is not a player for another
product's content.

## 2. Why it can sell

- **Nobody owns the slot.** Wallpaper Engine is Windows-only. linux-wallpaperengine only plays
  content made for Wallpaper Engine, and only partly. Hanabi (GNOME, video only), mpvpaper and swww
  are hobby tools with no catalogue. *Open: survey the field properly before the store page goes up.*
- **The Steam Deck is a Linux desktop.** Desktop mode is KDE Plasma on Wayland, so the
  layer-shell backend covers the Deck from the start. Most Linux Steam users are Deck owners.
- **The money is in the catalogue.** A renderer is not a product. The Workshop, the tools for
  making wallpapers and a strong set of first-party wallpapers are. Every phase below is judged by
  how fast it gets creators and content in.

## 3. Pillars, in priority order

1. **Free when unseen.** A wallpaper you can't see costs 0% GPU. Frames are driven by the
   compositor's frame callbacks, so an occluded surface doesn't render. There's an fps cap and a
   battery policy. This is the first thing reviews check.
2. **It looks incredible out of the box.** The first-party set comes from ficciones' ambients
   (braille dots, phosphor decay, the amber/cyan/lilac palette): a house style nobody else has.
3. **Making wallpapers is easy.** A wallpaper is a folder: `wallpaper.toml` plus a shader and its
   assets. That folder is the Workshop item, and `aleph render` makes its preview image.
4. **Every desktop.** Hyprland, Sway, KDE (Deck), niri and COSMIC through wlr-layer-shell. X11
   through a `_NET_WM_WINDOW_TYPE_DESKTOP` window. GNOME through a Shell extension, because GNOME
   has no layer-shell.

## 4. Architecture

One Rust binary, `aleph`.

- **Renderer:** wgpu (Vulkan, with GL as the fallback). A wallpaper kind is a set of render passes
  drawn into one output surface.
- **Front ends:** `run` (the desktop, one layer surface per output), `render` (headless: one frame
  at time *t* to a PNG, used for previews, Workshop thumbnails and the test suite). A picker UI
  comes later and drives `run` over a socket.
- **Wallpaper kinds**, in the order they ship:
  1. `shader`: a WGSL fragment function `wallpaper(px, res, t) -> vec4f`. *(slice 1)*
  2. `shader` + feedback: the previous frame as a texture, for state such as trails, automata, and
     the Lorenz port. *(slice 2)*
  3. `video`: hardware decode through GStreamer/VA-API, looped.
  4. `scene`: 2D layers, parallax, particles, audio-reactive parameters. This is what Wallpaper
     Engine's most popular content is, and the reason to build an editor.
  5. Shadertoy-dialect GLSL (`mainImage`) through naga's GLSL front end, so shader artists can
     bring what they already know. Every item still needs its own licence: Shadertoy's default
     licence is non-commercial.
  - Web wallpapers are **out**: shipping a browser engine per wallpaper is exactly the weight pillar
    1 forbids.
- **Why Rust:** wgpu and smithay-client-toolkit are mature, and the Steamworks SDK (C++) has a
  maintained binding (`steamworks` crate). It's also one static-ish binary that the Steam runtime
  can carry.

## 5. Steam

- **Runtime:** the binary runs inside Steam Linux Runtime 3 ("sniper", Debian 11, glibc 2.31).
  It has to be built against glibc ≤ 2.31: `cargo zigbuild --target x86_64-unknown-linux-gnu.2.31`,
  or the sniper SDK container. An Arch-built binary (glibc 2.44) won't load there. Slice 1 checks
  that a Wayland layer surface and the GPU both work from inside the container.
- **App type:** Software, $100 Steam Direct fee. Release through Early Access once the Workshop
  works.
- **Workshop:** Steamworks UGC. Subscribe, download, and the item shows up in the picker. Uploading
  happens from the picker, with the `render` preview as the item image.
- **Login start:** a wallpaper has to come up at login without Steam open. `aleph` installs an XDG
  autostart entry pointing at its Steam install. *Open: whether Workshop updates need Steam
  running, and how DRM-free that makes us.*

## 6. Phases

| | Ships | Proven by |
|-|-|-|
| S1 | shader kind; `run` on the layer shell, every output, frame-callback driven; `render` to PNG; first-party `rain`; the sniper check | `cargo test` (golden render); sniper run; grim screenshot of the live layer |
| S2 | feedback pass; Lorenz port; fps cap; config file with a wallpaper per output; fullscreen pause measured | frame counter under a fullscreen window |
| S3 | video kind | 4K loop CPU/GPU budget |
| S4 | picker UI + socket control | |
| S5 | KDE/Deck verified, X11, GNOME extension | runs on a Deck |
| S6 | Steamworks: app, depot built in sniper, Workshop subscribe + upload | Workshop round trip |
| S7 | scene kind + editor → Early Access | |

## 7. Open decisions (the human's)

- **Licence.** The repo is public and has no LICENSE, so it's all rights reserved. Options: keep the
  source available and sell on Steam (most protective), GPL-3 (others may rebuild, but must share),
  or MIT. This decides whether a free fork can undercut the store build.
- **Name.** "Aleph" may collide on the store. Check before buying the app slot.
- **Price.** Wallpaper Engine sells at $3.99–4.99.
- **GNOME:** whether it's in scope for Early Access or comes after.
