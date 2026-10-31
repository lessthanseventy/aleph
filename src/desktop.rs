use crate::gpu::{self, Renderer, Target, Uniforms};
use crate::toplevels::Toplevels;
use crate::wallpaper::{self, Wallpaper};
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
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
};
use std::ptr::NonNull;
use std::time::{Duration, Instant, SystemTime};
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
    start(wp, false)
}

/// The wallpaper in a window, reloaded whenever a file in its folder changes. A version that
/// does not compile is reported and the last good one stays up.
pub fn dev(wp: Wallpaper) -> Result<()> {
    start(wp, true)
}

fn start(wp: Wallpaper, dev: bool) -> Result<()> {
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
        dev,
        stamp: None,
        scanned: Instant::now(),
    };
    if dev {
        app.stamp = wallpaper::stamp(&app.wp.dir);
        let xdg = XdgShell::bind(&globals, &qh).context("no xdg_wm_base")?;
        let window = xdg.create_window(
            app.compositor.create_surface(&qh),
            WindowDecorations::ServerDefault,
            &qh,
        );
        window.set_title(format!("aleph dev: {}", app.wp.name));
        window.set_app_id("aleph.dev");
        window.commit();
        let surface = app.gpu_surface(window.wl_surface())?;
        app.screens
            .push(Screen::new("dev".into(), None, surface, Shell::Window(window), 1, 0.0));
    }
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

enum Shell {
    Layer(LayerSurface),
    Window(Window),
}

impl Shell {
    fn wl(&self) -> &wl_surface::WlSurface {
        match self {
            Shell::Layer(l) => l.wl_surface(),
            Shell::Window(w) => w.wl_surface(),
        }
    }
}

struct Screen {
    name: String,
    /// none for the dev window
    output: Option<wl_output::WlOutput>,
    // dropped before `shell`: the wgpu surface draws into its wl_surface
    surface: wgpu::Surface<'static>,
    shell: Shell,
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

impl Screen {
    fn new(
        name: String,
        output: Option<wl_output::WlOutput>,
        surface: wgpu::Surface<'static>,
        shell: Shell,
        scale: i32,
        seed: f32,
    ) -> Self {
        Screen {
            name,
            output,
            surface,
            shell,
            target: None,
            size: (0, 0),
            scale,
            seed,
            last: None,
            paused: false,
            waiting: false,
        }
    }
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
    /// `aleph dev`: one window, no layers, and the folder watched
    dev: bool,
    stamp: Option<SystemTime>,
    scanned: Instant,
}

impl App {
    fn screen(&mut self, s: &wl_surface::WlSurface) -> Option<usize> {
        self.screens.iter().position(|x| x.shell.wl() == s)
    }

    fn gpu_surface(&self, wl: &wl_surface::WlSurface) -> Result<wgpu::Surface<'static>> {
        let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr() as *mut _).unwrap(),
        ));
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(wl.id().as_ptr() as *mut _).unwrap(),
        ));
        // SAFETY: the display outlives every surface, and each Screen drops its surface first
        Ok(unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(display),
                    raw_window_handle: window,
                })
        }?)
    }

    /// The folder changed: load it again and swap the pipeline in if it builds.
    fn reload(&mut self) {
        let wp = match wallpaper::load(&self.wp.dir) {
            Ok(wp) => wp,
            Err(e) => return eprintln!("aleph: {e:#}"),
        };
        let Some(g) = &mut self.gpu else {
            self.wp = wp;
            return;
        };
        match Renderer::new(&g.device, &wp, g.format) {
            Ok(r) => {
                for s in &mut self.screens {
                    s.target = Some(r.target(&g.device));
                    s.last = None;
                }
                g.renderer = r;
                eprintln!("aleph: reloaded {}", wp.name);
                self.wp = wp;
            }
            Err(e) => eprintln!("aleph: {e:#}\naleph: still showing the last version that built"),
        }
    }

    /// Size the surface the compositor gave `wl` and, the first time, start its frames.
    fn configured(&mut self, wl: &wl_surface::WlSurface, w: u32, h: u32, qh: &QueueHandle<Self>) {
        let Some(i) = self.screen(wl) else { return };
        let first = self.screens[i].target.is_none();
        self.screens[i].size = (w.max(1), h.max(1));
        if let Err(e) = self.configure_surface(i) {
            eprintln!("aleph: {e:#}");
            std::process::exit(1);
        }
        self.screens[i].last = None;
        if first {
            self.tick(i, qh);
        }
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
        s.shell.wl().set_buffer_scale(s.scale);
        s.target = Some(target);
        Ok(())
    }

    /// Pause the outputs a focused fullscreen window now covers; restart the ones it left.
    fn recheck(&mut self, qh: &QueueHandle<Self>) {
        for i in 0..self.screens.len() {
            let s = &mut self.screens[i];
            let covered = s.output.as_ref().is_some_and(|o| self.toplevels.covers(o));
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
        let now = Instant::now();
        if self.dev && now - self.scanned > Duration::from_millis(250) {
            self.scanned = now;
            let stamp = wallpaper::stamp(&self.wp.dir);
            if stamp != self.stamp {
                self.stamp = stamp;
                self.reload();
            }
        }
        let step = Duration::from_secs_f32(1.0 / self.wp.fps);
        let s = &mut self.screens[i];
        if s.paused {
            return;
        }
        let wl = s.shell.wl().clone();
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
        if self.dev {
            return;
        }
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
        let surface = match self.gpu_surface(layer.wl_surface()) {
            Ok(s) => s,
            Err(e) => return eprintln!("aleph: no GPU surface for an output: {e}"),
        };
        let info = self.outputs.info(&output);
        let scale = info.as_ref().map_or(1, |i| i.scale_factor);
        let name = info.and_then(|i| i.name).unwrap_or_else(|| "?".into());
        self.seeds += 1;
        let seed = self.seeds as f32 * 17.31;
        self.screens.push(Screen::new(
            name,
            Some(output),
            surface,
            Shell::Layer(layer),
            scale,
            seed,
        ));
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.screens.retain(|s| s.output.as_ref() != Some(&output));
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.screens.retain(|s| s.shell.wl() != layer.wl_surface());
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        c: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.configured(layer.wl_surface(), c.new_size.0, c.new_size.1, qh);
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        std::process::exit(0);
    }

    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, w: &Window, c: WindowConfigure, _: u32) {
        let (width, height) = c.new_size;
        let size = (width.map_or(960, |v| v.get()), height.map_or(540, |v| v.get()));
        // a configure with an unchanged size (focus, a state change) needs no new swapchain
        if let Some(i) = self.screen(w.wl_surface())
            && self.screens[i].target.is_some()
            && self.screens[i].size == size
        {
            return;
        }
        self.configured(w.wl_surface(), size.0, size.1, qh);
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
