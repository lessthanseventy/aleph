use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;

/// A wallpaper is a folder: `wallpaper.toml` and the files it names. That folder is also what a
/// Workshop item holds.
#[derive(Deserialize)]
struct Manifest {
    name: String,
    kind: String,
    entry: String,
    /// The rate the wallpaper changes at; a frame between two of its steps would be a copy.
    #[serde(default = "default_fps")]
    fps: f32,
}

fn default_fps() -> f32 {
    30.0
}

pub struct Wallpaper {
    pub name: String,
    pub fps: f32,
    /// The whole WGSL module: the preamble, then the wallpaper's own source.
    pub source: String,
}

pub fn load(dir: &Path) -> Result<Wallpaper> {
    let toml_path = dir.join("wallpaper.toml");
    let text = std::fs::read_to_string(&toml_path).with_context(|| format!("reading {}", toml_path.display()))?;
    let m: Manifest = toml::from_str(&text).with_context(|| format!("parsing {}", toml_path.display()))?;
    if m.kind != "shader" {
        bail!("{}: kind {:?} is not one aleph plays yet (only \"shader\")", toml_path.display(), m.kind);
    }
    if !(m.fps > 0.0) {
        bail!("{}: fps must be above 0", toml_path.display());
    }
    let entry = dir.join(&m.entry);
    let own = std::fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
    Ok(Wallpaper { name: m.name, fps: m.fps, source: format!("{}\n{}", include_str!("preamble.wgsl"), own) })
}
