// Rain on dark water: drops land at random, their rings spread flat and thin out. Drawn as
// braille dots with no state: each drop slot replays a ring per period, placed by a hash of the
// slot and the period's number.

const PITCH = 4.5;   // a dot's cell in pixels, as braille in a 9×18 px character cell
const LIT = 0.64;    // how much of its cell a dot lights
const MAX_DROPS = 64u;
const SPEED = 12.0;  // ring growth, dots a second
const TICK = 15.0;   // the field changes this many times a second; the manifest's fps matches
const CYAN = vec3f(0.2, 0.78, 1.0);      // #33C7FF
const LILAC = vec3f(0.725, 0.541, 1.0);  // #B98AFF

fn hash(x: u32) -> u32 {
    let v = (x * 747796405u + 2891336453u);
    let w = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return (w >> 22u) ^ w;
}

fn rand(a: u32, b: u32) -> f32 {
    return f32(hash(a ^ hash(b))) / 4294967295.0;
}

fn wallpaper(px: vec2f) -> vec4f {
    let black = vec4f(0.0, 0.0, 0.0, 1.0);
    if (any(abs(fract(px / PITCH) - 0.5) > vec2f(LIT * 0.5))) {
        return black;
    }
    let dot = floor(px / PITCH) + 0.5;
    let dims = floor(aleph.res / PITCH);
    let seed = u32(aleph.seed * 1000.0);
    let tick = u32(floor(aleph.time * TICK));
    let t = f32(tick) / TICK;
    // as many drops as the screen has room for: a fixed dozen reads as empty on a big one
    let n = min(MAX_DROPS, u32(dims.x * dims.y / 3000.0) + 4u);
    let dot_id = hash(u32(dot.x) + u32(dot.y) * 8191u);
    var best = 0.0;
    var colour = vec3f(0.0);
    for (var i = 0u; i < n; i++) {
        let period = 3.0 + 3.0 * rand(i, seed);
        let k = t / period + rand(i, seed + 1u);
        let id = hash(i * 7919u + u32(floor(k)) * 104729u + seed);
        let reach = 16.0 + 40.0 * rand(id, 2u);
        let r = fract(k) * period * SPEED;
        if (r >= reach) {
            continue;
        }
        let centre = vec2f(rand(id, 3u) * dims.x, dims.y * (0.1 + 0.85 * rand(id, 4u)));
        let d = dot - centre;
        if (abs(length(vec2f(d.x, d.y / 0.45)) - r) > 0.75) {
            continue;
        }
        let life = 1.0 - r / reach;
        // a ring thins as it spreads: each of its dots lights with the ring's life
        if (rand(dot_id, tick * 131u + i) > life * 0.9) {
            continue;
        }
        if (life > best) {
            best = life;
            colour = select(LILAC, CYAN, life > 0.5);
        }
    }
    return vec4f(colour, 1.0);
}
