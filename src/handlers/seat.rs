use smithay::{
    delegate_seat,
    input::{pointer::CursorImageStatus, Seat, SeatHandler, SeatState},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
};

use crate::state::State;

impl<BackendData: 'static> SeatHandler for State<BackendData> {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.cursor_status = image;
    }
}
delegate_seat!(@<BackendData: 'static> State<BackendData>);
