use std::time::Instant;

use smithay::{
    desktop::{Space, Window, WindowSurfaceType},
    input::{pointer::PointerHandle, Seat, SeatState},
    reexports::{calloop::LoopSignal, wayland_server::protocol::wl_surface::WlSurface},
    utils::{Logical, Point},
    wayland::{
        compositor::CompositorState, data_device::DataDeviceState, output::OutputManagerState,
        shell::xdg::XdgShellState, shm::ShmState,
    },
};

pub struct State {
    pub start_time: Instant,
    pub loop_signal: LoopSignal,

    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub xdg_shell_state: XdgShellState,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<Self>,

    pub space: Space<Window>,
}

impl State {
    pub fn surface_under_pointer(
        &self,
        pointer: &PointerHandle<Self>,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        let pos = pointer.current_location();
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, point)| (surface, point + location))
            })
    }
}
