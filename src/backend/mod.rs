mod drm;
mod winit;

use crate::{
    cursor,
    state::{self},
};

use self::{drm::run_drm_backend, winit::run_winit_backend};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    CursorLoadError(cursor::Error),

    #[error("Failed to create Wayland display")]
    DisplayCreateFailure,

    #[error("Failed to create drm surface")]
    DrmSurfaceCreateFailure,

    #[error("Failed to create event loop")]
    EventLoopCreateFailure,

    #[error("Failed to create gbm surface")]
    GbmSurfaceCreateFailure,

    #[error("Failed to create GPU manager")]
    GpuManagerCreateFailure,

    #[error("No GPU found")]
    NoGPUFound,

    #[error("Failed to get the path of the primary GPU")]
    PrimaryGPUGetFailure,

    #[error("Failed to initialize session")]
    SessionInitFailure,

    #[error("Failed to create Wayland socket")]
    SocketCreateFailure,

    #[error("Failed to insert source into event loop")]
    SourceInsertFailure,

    #[error("{0}")]
    StateCreateFailure(state::Error),

    #[error("Failed to assign seat to the libinput context")]
    LibinputAssignSeatFailure,

    #[error("Failed to initialize Udev backend")]
    UdevInitFailure,
}

pub fn run_backend_auto() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
        if wayland_display != "" {
            run_winit_backend()?;
        } else {
            run_drm_backend()?;
        }
    } else {
        run_drm_backend()?;
    }

    Ok(())
}
