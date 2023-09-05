use smithay::{
    reexports::wayland_server::{backend, Display},
    wayland::compositor::CompositorClientState,
};

use crate::state::State;

pub struct Data<BackendData: 'static> {
    pub display: Display<State<BackendData>>,
    pub state: State<BackendData>,
}

#[derive(Default)]
pub struct ClientData {
    pub compositor_state: CompositorClientState,
}

impl backend::ClientData for ClientData {}
