use smithay::{
    reexports::wayland_server::{backend, Display},
    wayland::compositor::CompositorClientState,
};

use crate::state::State;

/// Event loop data.
pub struct Data<BackendData: 'static> {
    pub display: Display<State<BackendData>>,
    pub state: State<BackendData>,
}

// Data of a client.
#[derive(Default)]
pub struct ClientData {
    pub compositor_state: CompositorClientState,
}

impl backend::ClientData for ClientData {}
