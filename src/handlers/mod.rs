use smithay::{
    delegate_data_device, delegate_output, delegate_shm,
    reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    wayland::{
        buffer::BufferHandler,
        data_device::{
            ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
        },
        shm::{ShmHandler, ShmState},
    },
};

use crate::state::State;

mod compositor;
mod seat;
mod xdg_shell;

impl<BackendData> BufferHandler for State<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl<BackendData> ShmHandler for State<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}
delegate_shm!(@<BackendData: 'static> State<BackendData>);
delegate_output!(@<BackendData: 'static> State<BackendData>);

impl<BackendData: 'static> ClientDndGrabHandler for State<BackendData> {}
impl<BackendData: 'static> ServerDndGrabHandler for State<BackendData> {}

impl<BackendData: 'static> DataDeviceHandler for State<BackendData> {
    type SelectionUserData = ();

    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}
delegate_data_device!(@<BackendData: 'static> State<BackendData>);
