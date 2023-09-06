use std::time::Instant;

use smithay::{
    desktop::{Space, Window, WindowSurfaceType},
    input::{pointer::PointerHandle, Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{EventLoop, LoopSignal},
        wayland_server::{protocol::wl_surface::WlSurface, Display},
    },
    utils::{Logical, Point},
    wayland::{
        compositor::CompositorState, data_device::DataDeviceState, output::OutputManagerState,
        shell::xdg::XdgShellState, shm::ShmState,
    },
};

use crate::data::Data;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to add keyboard")]
    KeyboardAddFailure,
}

pub struct State<BackendData: 'static> {
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

    pub backend_data: BackendData,
}

impl<BackendData> State<BackendData> {
    pub fn new(
        display: &Display<Self>,
        event_loop: &mut EventLoop<Data<BackendData>>,
        backend_data: BackendData,
    ) -> Result<Self, Error> {
        // Get the display handle. Again: it is just related to the Wayland protocol and has nothing to
        // do with the backend.
        let dh = display.handle();

        // Used to compose.
        let compositor_state = CompositorState::new::<Self>(&dh);
        // Used to create shared memory buffers.
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat = seat_state.new_wl_seat(&dh, "alioth");
        // FIXME: Implement hot-plug
        seat.add_keyboard(Default::default(), 200, 200)
            .or_else(|_| {
                tracing::error!("Failed to add keyboard");
                Err(Error::KeyboardAddFailure)
            })?;
        seat.add_pointer();

        let space = Space::default();

        // Pack the state.
        let state = State {
            start_time: Instant::now(),
            loop_signal: event_loop.get_signal(),

            compositor_state,
            shm_state,
            output_manager_state,
            xdg_shell_state,
            seat_state,
            data_device_state,
            seat,

            space,

            backend_data,
        };

        Ok(state)
    }

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

    pub fn map_output_on_the_right(&mut self, output: Output) {
        let x = self.space.outputs().fold(0, |sum, output| {
            sum + self.space.output_geometry(output).unwrap().size.w
        });
        self.space.map_output(&output, (x, 0));
    }
}
