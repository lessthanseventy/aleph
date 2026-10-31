use wayland_client::protocol::wl_output::WlOutput;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    Event, State, ZwlrForeignToplevelHandleV1,
};

#[derive(Default)]
struct Toplevel {
    outputs: Vec<WlOutput>,
    activated: bool,
    fullscreen: bool,
    // a state event lands at the handle's next `done`
    pending: Option<(bool, bool)>,
}

/// The desktop's windows, through wlr-foreign-toplevel, to know which outputs a fullscreen window
/// hides: compositors (Sway at least) keep sending a covered background layer frame callbacks.
#[derive(Default)]
pub struct Toplevels {
    all: Vec<(ZwlrForeignToplevelHandleV1, Toplevel)>,
}

impl Toplevels {
    /// Only the focused window counts: one fullscreen on a workspace out of sight still reports
    /// its output, and a wallpaper frozen in plain view is worse than one drawn unseen.
    pub fn covers(&self, o: &WlOutput) -> bool {
        self.all
            .iter()
            .any(|(_, t)| t.activated && t.fullscreen && t.outputs.contains(o))
    }

    pub fn add(&mut self, h: ZwlrForeignToplevelHandleV1) {
        self.all.push((h, Toplevel::default()));
    }

    /// Apply one event; true when the covered outputs may have changed.
    pub fn event(&mut self, h: &ZwlrForeignToplevelHandleV1, ev: Event) -> bool {
        let Some(i) = self.all.iter().position(|(x, _)| x == h) else {
            return false;
        };
        let t = &mut self.all[i].1;
        match ev {
            Event::OutputEnter { output } => t.outputs.push(output),
            Event::OutputLeave { output } => t.outputs.retain(|o| o != &output),
            Event::State { state } => {
                let has = |s: State| {
                    state
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|c| u32::from_ne_bytes(*c) == s as u32)
                };
                t.pending = Some((has(State::Activated), has(State::Fullscreen)));
            }
            Event::Done => {
                if let Some((a, f)) = t.pending.take() {
                    (t.activated, t.fullscreen) = (a, f);
                }
                return true;
            }
            Event::Closed => {
                self.all.remove(i).0.destroy();
                return true;
            }
            _ => {}
        }
        false
    }
}
