use smithay::{
    reexports::wayland_server::{backend, Display},
    wayland::compositor::CompositorClientState,
};

use crate::state::State;

pub struct Data {
    pub display: Display<State>,
    pub state: State,
}

#[derive(Default)]
pub struct ClientData {
    pub compositor_state: CompositorClientState,
}

impl backend::ClientData for ClientData {}
