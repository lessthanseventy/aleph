use crate::gpu::{self, Renderer, Target, Uniforms};
use crate::toplevels::Toplevels;
use crate::wallpaper::Wallpaper;
use anyhow::{Context, Result};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
        },
    },
};
use std::ptr::NonNull;
use std::time::{Duration, Instant};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, event_created_child,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self as handle, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self as manager, ZwlrForeignToplevelManagerV1},
};

/// The wallpaper as the background layer of every output, until killed. Frames are paced by the
/// compositor's frame callbacks, so an output it does not show (covered, off) costs no frames.
pub fn run(wp: Wallpaper) -> Result<()> {
    let conn = Connection::connect_to_env().context("no Wayland display (is WAYLAND_DISPLAY set?)")?;
    let (globals, mut queue) = registry_queue_init(&conn)?;
    let qh = queue.handle();
    let mut app = App {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh).context("no wl_compositor")?,
        layer_shell: LayerShell::bind(&globals, &qh).context("this compositor has no wlr-layer-shell")?,
        conn: conn.clone(),
        instance: gpu::instance(),
        gpu: None,
        wp,
        start: Instant::now(),
        screens: Vec::new(),
        seeds: 0,
        stats: std::env::var_os("ALEPH_STATS").is_some(),
        toplevels: Toplevels::default(),
        // without it (KDE, GNOME), every output draws whatever covers it
        _toplevel_manager: globals.bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ()).ok(),
    };
    loop {
        queue.blocking_dispatch(&mut app)?;
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    renderer: Renderer,
}

struct Screen {
    name: String,
    output: wl_output::WlOutput,
    // dropped before `layer`: the wgpu surface draws into the layer's wl_surface
    surface: wgpu::Surface<'static>,
    layer: LayerSurface,
    target: Option<Target>,
    size: (u32, u32),
    scale: i32,
    seed: f32,
    last: Option<Instant>,
    /// a focused fullscreen window covers the output: no frames, no frame callbacks
    paused: bool,
    /// a frame callback is asked for and has not come; never two in flight
    waiting: bool,
}

struct App {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    conn: Connection,
    instance: wgpu::Instance,
    gpu: Option<Gpu>,
    wp: Wallpaper,
    start: Instant,
    screens: Vec<Screen>,
    seeds: u32,
    /// ALEPH_STATS set: a stderr line per frame drawn, for measuring what a wallpaper costs
    stats: bool,
    toplevels: Toplevels,
    _toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
}

impl App {
    fn screen(&mut self, s: &wl_surface::WlSurface) -> Option<usize> {
        self.screens.iter().position(|x| x.layer.wl_surface() == s)
    }

    /// The device, made on the first surface it must draw to; every output shares it.
    fn gpu_for(&mut self, i: usize) -> Result<()> {
        if self.gpu.is_none() {
            let surface = &self.screens[i].surface;
            let adapter = pollster::block_on(self.instance.request_adapter(&gpu::adapter_options(Some(surface))))?;
            let (device, queue) = gpu::device(&adapter)?;
            let caps = surface.get_capabilities(&adapter);
            // a non-sRGB format: the shader's colours go to the screen as written, like a hex code
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| !f.is_srgb())
                .unwrap_or(caps.formats[0]);
            let renderer = Renderer::new(&device, &self.wp, format)?;
            self.gpu = Some(Gpu {
                device,
                queue,
                format,
                renderer,
            });
        }
        Ok(())
    }

    fn configure_surface(&mut self, i: usize) -> Result<()> {
        self.gpu_for(i)?;
        let g = self.gpu.as_ref().unwrap();
        let s = &self.screens[i];
        let (w, h) = (s.size.0 * s.scale as u32, s.size.1 * s.scale as u32);
        s.surface.configure(
            &g.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: g.format,
                color_space: Default::default(),
                width: w,
                height: h,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 1,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
            },
        );
        let target = g.renderer.target(&g.device);
        let s = &mut self.screens[i];
        s.layer.wl_surface().set_buffer_scale(s.scale);
        s.target = Some(target);
        Ok(())
    }

    /// Pause the outputs a focused fullscreen window now covers; restart the ones it left.
    fn recheck(&mut self, qh: &QueueHandle<Self>) {
        for i in 0..self.screens.len() {
            let s = &mut self.screens[i];
            let covered = self.toplevels.covers(&s.output);
            if covered == s.paused {
                continue;
            }
            s.paused = covered;
            if self.stats {
                eprintln!("aleph: {} {}", if covered { "paused" } else { "resumed" }, s.name);
            }
            if !covered && !s.waiting && s.target.is_some() {
                s.last = None;
                self.tick(i, qh);
            }
        }
    }

    /// Draw output `i` if the wallpaper has moved on since its last frame; either way ask for the
    /// next frame callback. A paused output asks for none, so it costs no wakeups at all.
    fn tick(&mut self, i: usize, qh: &QueueHandle<Self>) {
        let step = Duration::from_secs_f32(1.0 / self.wp.fps);
        let now = Instant::now();
        let s = &mut self.screens[i];
        if s.paused {
            return;
        }
        let wl = s.layer.wl_surface().clone();
        wl.frame(qh, FrameCallbackData(wl.clone()));
        s.waiting = true;
        if s.last.is_some_and(|l| now - l < step) {
            wl.commit();
            return;
        }
        s.last = Some(now);
        let (Some(g), Some(target)) = (&self.gpu, &s.target) else {
            return wl.commit();
        };
        let frame = match s.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            other => {
                eprintln!("aleph: no frame to draw into ({other:?})");
                return wl.commit();
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = g.device.create_command_encoder(&Default::default());
        let res = [(s.size.0 * s.scale as u32) as f32, (s.size.1 * s.scale as u32) as f32];
        g.renderer.draw(
            &g.queue,
            &mut enc,
            &view,
            target,
            Uniforms {
                res,
                time: (now - self.start).as_secs_f32(),
                seed: s.seed,
            },
        );
        g.queue.submit([enc.finish()]);
        g.queue.present(frame);
        if self.stats {
            eprintln!("aleph: frame {}", s.name);
        }
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let wl = self.compositor.create_surface(qh);
        let layer = self
            .layer_shell
            .create_layer_surface(qh, wl, Layer::Background, Some("aleph"), Some(&output));
        layer.set_anchor(Anchor::all());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        // an empty input region: clicks fall through to the desktop
        if let Ok(region) = smithay_client_toolkit::compositor::Region::new(&self.compositor) {
            layer.wl_surface().set_input_region(Some(region.wl_region()));
        }
        layer.commit();
        let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr() as *mut _).unwrap(),
        ));
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(layer.wl_surface().id().as_ptr() as *mut _).unwrap(),
        ));
        let surface = match unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(display),
                    raw_window_handle: window,
                })
        } {
            Ok(s) => s,
            Err(e) => return eprintln!("aleph: no GPU surface for an output: {e}"),
        };
        let info = self.outputs.info(&output);
        let scale = info.as_ref().map_or(1, |i| i.scale_factor);
        let name = info.and_then(|i| i.name).unwrap_or_else(|| "?".into());
        self.seeds += 1;
        self.screens.push(Screen {
            name,
            output,
            surface,
            layer,
            target: None,
            size: (0, 0),
            scale,
            seed: self.seeds as f32 * 17.31,
            last: None,
            paused: false,
            waiting: false,
        });
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.screens.retain(|s| s.output != output);
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.screens.retain(|s| &s.layer != layer);
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        c: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(i) = self.screen(layer.wl_surface()) else {
            return;
        };
        let first = self.screens[i].target.is_none();
        self.screens[i].size = (c.new_size.0.max(1), c.new_size.1.max(1));
        if let Err(e) = self.configure_surface(i) {
            eprintln!("aleph: {e:#}");
            std::process::exit(1);
        }
        self.screens[i].last = None;
        if first {
            self.tick(i, qh);
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, s: &wl_surface::WlSurface, factor: i32) {
        let Some(i) = self.screen(s) else { return };
        if self.screens[i].scale != factor && self.screens[i].target.is_some() {
            self.screens[i].scale = factor;
            if let Err(e) = self.configure_surface(i) {
                eprintln!("aleph: {e:#}");
            }
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, s: &wl_surface::WlSurface, _: u32) {
        if let Some(i) = self.screen(s) {
            self.screens[i].waiting = false;
            self.tick(i, qh);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        ev: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let manager::Event::Toplevel { toplevel } = ev {
            app.toplevels.add(toplevel);
        }
    }

    event_created_child!(App, ZwlrForeignToplevelManagerV1, [manager::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ())]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for App {
    fn event(
        app: &mut Self,
        h: &ZwlrForeignToplevelHandleV1,
        ev: handle::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if app.toplevels.event(h, ev) {
            app.recheck(qh);
        }
    }
}

delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(App);
