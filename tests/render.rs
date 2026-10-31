use aleph::{headless, wallpaper};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static GPU: LazyLock<headless::Headless> = LazyLock::new(|| headless::Headless::new().unwrap());

const CYAN: [u8; 3] = [0x33, 0xC7, 0xFF];
const LILAC: [u8; 3] = [0xB9, 0x8A, 0xFF];

fn rain() -> wallpaper::Wallpaper {
    wallpaper::load(Path::new("wallpapers/rain")).unwrap()
}

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, text) in files {
        std::fs::write(dir.join(f), text).unwrap();
    }
    dir
}

#[test]
fn rain_draws_its_palette_on_black_the_same_every_time() {
    let a = GPU.frame(&rain(), 640, 400, 20.0, 0.0).unwrap();
    let b = GPU.frame(&rain(), 640, 400, 20.0, 0.0).unwrap();
    assert_eq!(a, b, "the same time and seed must draw the same frame");
    let lit = a.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count();
    assert!(lit > 100, "only {lit} lit pixels");
    for p in a.chunks(4) {
        // within one unit of the hex: the GPU may round the f32 colour either way
        let near = |c: [u8; 3]| p[..3].iter().zip(c).all(|(x, y)| x.abs_diff(y) <= 1);
        assert!(
            p[..3] == [0, 0, 0] || near(CYAN) || near(LILAC),
            "off-palette pixel {p:?}"
        );
    }
}

#[test]
fn rain_moves_and_differs_per_output() {
    let at = |t, seed| GPU.frame(&rain(), 320, 200, t, seed).unwrap();
    assert_ne!(at(20.0, 0.0), at(21.0, 0.0));
    assert_ne!(at(20.0, 0.0), at(20.0, 17.31));
}

#[test]
fn a_shader_that_does_not_compile_names_the_error() {
    let dir = scratch(
        "broken",
        &[
            (
                "wallpaper.toml",
                "name = \"broken\"\nkind = \"shader\"\nentry = \"w.wgsl\"\n",
            ),
            ("w.wgsl", "fn wallpaper(px: vec2f) -> vec4f { return nope; }"),
        ],
    );
    let err = GPU
        .frame(&wallpaper::load(&dir).unwrap(), 16, 16, 0.0, 0.0)
        .unwrap_err();
    assert!(format!("{err:#}").contains("nope"), "{err:#}");
}

#[test]
fn a_kind_aleph_cannot_play_is_refused_at_load() {
    let dir = scratch(
        "video",
        &[("wallpaper.toml", "name = \"v\"\nkind = \"video\"\nentry = \"v.mp4\"\n")],
    );
    let err = wallpaper::load(&dir).err().expect("a video manifest must not load yet");
    assert!(format!("{err:#}").contains("\"video\""), "{err:#}");
}
