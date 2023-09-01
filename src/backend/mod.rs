mod drm;
mod winit;

use crate::state::{self};

use self::winit::run_winit_backend;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create Wayland display")]
    DisplayCreateFailure,

    #[error("Failed to create event loop")]
    EventLoopCreateFailure,

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

    #[error("Failed to initialize Udev backend")]
    UdevInitFailure,
}

pub fn run_backend_auto() -> Result<(), Box<dyn std::error::Error>> {
    run_winit_backend()?;
    Ok(())
}
