mod winit;

use smithay::reexports::{calloop::EventLoop, wayland_server::DisplayHandle};

use crate::{data::Data, state::State};

use self::winit::init_winit_backend;

pub enum Backend {
    Winit,
}

pub fn init_backend_auto(
    event_loop: &mut EventLoop<Data>,
    dh: &DisplayHandle,
    state: &mut State,
) -> Result<Backend, Box<dyn std::error::Error>> {
    init_winit_backend(event_loop, &dh, state)?;
    Ok(Backend::Winit)
}
