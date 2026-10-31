use crate::wallpaper::Wallpaper;
use anyhow::{Result, anyhow};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    pub res: [f32; 2],
    pub time: f32,
    pub seed: f32,
}

/// A wallpaper's pipeline, built once for a device and a surface format.
pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

/// What one output draws with: its own uniforms.
pub struct Target {
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

impl Renderer {
    /// A shader that does not compile is an error carrying the compiler's message.
    pub fn new(device: &wgpu::Device, wp: &Wallpaper, format: wgpu::TextureFormat) -> Result<Self> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&wp.name),
            source: wgpu::ShaderSource::Wgsl(wp.source.as_str().into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&wp.name),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("aleph_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("aleph_fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(anyhow!("{}: {e}", wp.name));
        }
        Ok(Self { pipeline, layout })
    }

    pub fn target(&self, device: &wgpu::Device) -> Target {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        Target { uniforms, bind }
    }

    pub fn draw(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, t: &Target, u: Uniforms) {
        queue.write_buffer(&t.uniforms, 0, bytemuck::bytes_of(&u));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &t.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
}

/// The integrated GPU where there is one: a wallpaper should never wake a discrete card.
pub fn adapter_options<'a>(surface: Option<&'a wgpu::Surface<'a>>) -> wgpu::RequestAdapterOptions<'a, 'a> {
    wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::LowPower, compatible_surface: surface, ..Default::default() }
}

pub fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    Ok(pollster::block_on(adapter.request_device(&Default::default()))?)
}
