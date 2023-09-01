use smithay::{
    backend::{
        drm::DrmNode,
        session::libseat::LibSeatSession,
        udev::{primary_gpu, UdevBackend},
    },
    reexports::{calloop::EventLoop, wayland_server::DisplayHandle},
};

use crate::state::State;

use super::Error;

pub fn init_drm_backend(
    event_loop: &mut EventLoop<State>,
    dh: &DisplayHandle,
    state: &mut State,
) -> Result<(), Error> {
    // Initialize session.
    let (session, notifier) = match LibSeatSession::new() {
        Ok(ret) => ret,
        Err(_) => {
            tracing::error!("Failed to initialize session");
            return Err(Error::SessionInitFailure);
        }
    };

    // Initialize the compositor.
    let primary_gpu = {
        let path = match primary_gpu(state.seat.name()) {
            Ok(path) => match path {
                Some(path) => path,
                None => {
                    tracing::error!("No GPU found");
                    return Err(Error::NoGPUFound);
                }
            },
            Err(_) => {
                tracing::error!("Failed to get primary GPU path");
                return Err(Error::PrimaryGPUGetFailure);
            }
        };
        match DrmNode::from_path(&path) {
            Ok(ret) => ret,
            Err(_) => {
                tracing::error!("No GPU found");
                return Err(Error::NoGPUFound);
            }
        }
    };
    tracing::info!("Using {} as primary GPU", primary_gpu);

    // Initialize the udev backend.
    let udev_backend = match UdevBackend::new(state.seat.name()) {
        Ok(ret) => ret,
        Err(_) => {
            tracing::error!("Failed to initialize Udev backend");
            return Err(Error::UdevInitFailure);
        }
    };

    Ok(())
}
