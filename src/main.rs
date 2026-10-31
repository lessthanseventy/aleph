use aleph::{desktop, headless, wallpaper};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

const USAGE: &str = "usage:
  aleph run WALLPAPER_DIR
      the wallpaper behind every output, until killed
  aleph dev WALLPAPER_DIR
      the wallpaper in a window, reloaded on every save; a shader error keeps the last good one
  aleph render WALLPAPER_DIR OUT.png [--size WxH] [--time SECONDS] [--seed N]
      one frame, headless (previews, Workshop images, tests)";

fn main() {
    if let Err(e) = cli(std::env::args().skip(1).collect()) {
        eprintln!("aleph: {e:#}");
        std::process::exit(1);
    }
}

fn cli(args: Vec<String>) -> Result<()> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["run", dir] => desktop::run(wallpaper::load(&PathBuf::from(dir))?),
        ["dev", dir] => desktop::dev(wallpaper::load(&PathBuf::from(dir))?),
        ["render", dir, out, rest @ ..] => {
            let (mut w, mut h, mut time, mut seed) = (1920, 1080, 10.0, 0.0);
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let v = it.next().with_context(|| format!("{flag} needs a value"))?;
                match *flag {
                    "--size" => {
                        let (a, b) = v.split_once('x').context("--size is WxH")?;
                        (w, h) = (a.parse()?, b.parse()?);
                    }
                    "--time" => time = v.parse()?,
                    "--seed" => seed = v.parse()?,
                    _ => bail!("unknown flag {flag}\n{USAGE}"),
                }
            }
            let wp = wallpaper::load(&PathBuf::from(dir))?;
            let rgba = headless::Headless::new()?.frame(&wp, w, h, time, seed)?;
            headless::write_png(&PathBuf::from(out), w, h, &rgba)
        }
        _ => bail!("{USAGE}"),
    }
}
