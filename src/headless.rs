use crate::gpu::{self, Renderer, Uniforms};
use crate::wallpaper::Wallpaper;
use anyhow::{Result, anyhow};

/// A GPU with no display. One per process: creating Vulkan instances on several threads at once
/// crashes the driver.
pub struct Headless {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Headless {
    pub fn new() -> Result<Self> {
        let adapter = pollster::block_on(gpu::instance().request_adapter(&gpu::adapter_options(None)))?;
        let (device, queue) = gpu::device(&adapter)?;
        Ok(Self { device, queue })
    }

    /// One frame of `wp` at `time`, as tightly packed RGBA8 rows.
    pub fn frame(&self, wp: &Wallpaper, w: u32, h: u32, time: f32, seed: f32) -> Result<Vec<u8>> {
        frame(&self.device, &self.queue, wp, w, h, time, seed)
    }
}

fn frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    wp: &Wallpaper,
    w: u32,
    h: u32,
    time: f32,
    seed: f32,
) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let r = Renderer::new(device, wp, format)?;
    let target = r.target(device);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let stride = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (stride * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    let view = tex.create_view(&Default::default());
    r.draw(
        queue,
        &mut enc,
        &view,
        &target,
        Uniforms {
            res: [w as f32, h as f32],
            time,
            seed,
        },
    );
    enc.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
        },
        tex.size(),
    );
    queue.submit([enc.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buf.slice(..).map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()?.map_err(|e| anyhow!("reading the frame back: {e}"))?;
    let mapped = buf.slice(..).get_mapped_range()?;
    Ok(mapped
        .chunks(stride as usize)
        .flat_map(|row| &row[..(w * 4) as usize])
        .copied()
        .collect())
}

pub fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    let mut e = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path)?), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()?.write_image_data(rgba)?;
    Ok(())
}
