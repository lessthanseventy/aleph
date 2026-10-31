// Prepended to every shader wallpaper. A wallpaper defines `fn wallpaper(px: vec2f) -> vec4f`:
// px is the pixel centre, origin top left; the colour out is sRGB-encoded, as written in hex.
struct Aleph {
    res: vec2f,  // the output, in pixels
    time: f32,   // seconds since the wallpaper started
    seed: f32,   // differs per output, so neighbours never show the same thing
}
@group(0) @binding(0) var<uniform> aleph: Aleph;

@vertex
fn aleph_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn aleph_fs(@builtin(position) p: vec4f) -> @location(0) vec4f {
    return wallpaper(p.xy);
}
