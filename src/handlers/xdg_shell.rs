use smithay::{
    delegate_xdg_shell,
    desktop::{PopupKind, Window},
    input::{
        pointer::{Focus, GrabStartData},
        Seat,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Rectangle, Serial},
    wayland::shell::xdg::{
        PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
    },
};

use crate::{
    grabs::{resize_grab::ResizeSurfaceGrab, MoveSurfaceGrab},
    state::State,
};

impl<BackendData> XdgShellHandler for State<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new(surface);
        self.space.map_element(window, (0, 0), true);
    }

    fn toplevel_destroyed(&mut self, _surface: ToplevelSurface) {
        self.popups.cleanup();
        self.space.refresh();
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.popups.track_popup(PopupKind::Xdg(surface)).ok();
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: WlSeat, serial: Serial) {
        let seat: Seat<Self> = Seat::from_resource(&seat).unwrap();
        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            // If check_grab() returns Some, there must exist a pointer.
            let pointer = seat.get_pointer().unwrap();

            let window = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == wl_surface)
                .unwrap()
                .clone();
            let initial_location = self.space.element_location(&window).unwrap();

            let grab = MoveSurfaceGrab {
                start_data,
                window,
                initial_location,
            };

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat: Seat<Self> = Seat::from_resource(&seat).unwrap();

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = if let Some(pointer) = seat.get_pointer() {
                pointer
            } else {
                return;
            };

            let window = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == wl_surface)
                .cloned()
                .unwrap();

            let initial_window_location = self.space.element_location(&window).unwrap();
            let initial_window_size = window.geometry().size;

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });
            surface.send_pending_configure();

            let grab = ResizeSurfaceGrab::start(
                start_data,
                window,
                edges.into(),
                Rectangle::from_loc_and_size(initial_window_location, initial_window_size),
            );

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {}
}
delegate_xdg_shell!(@<BackendData: 'static> State<BackendData>);

fn check_grab<BackendData>(
    seat: &Seat<State<BackendData>>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<GrabStartData<State<BackendData>>> {
    let pointer = seat.get_pointer()?;

    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}
