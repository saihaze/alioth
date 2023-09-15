use backend::run_backend_auto;
use data::{ClientData, Data};
use smithay::{reexports::calloop::EventLoop, wayland::socket::ListeningSocketSource};

use std::sync::Arc;

mod backend;
mod cursor;
mod data;
mod grabs;
mod handlers;
mod input;
mod state;
mod workspace;

/// Create a Unix socket for the Wayland server.
fn init_wayland_socket<BackendData>(
    event_loop: &mut EventLoop<Data<BackendData>>,
) -> Result<String, anyhow::Error> {
    // Create the socket.
    let socket = ListeningSocketSource::new_auto()?;
    // Get the socket name to be returned.
    let name = socket.socket_name().to_string_lossy().to_string();

    // Following delegate will be called whenever a new connection to the socket is established.
    // We insert the client to the display and then the display will handle it.
    event_loop
        .handle()
        .insert_source(socket, |stream, _, data| {
            match data
                .display
                .handle()
                .insert_client(stream, Arc::new(ClientData::default()))
            {
                Ok(_) => (),
                Err(_) => {
                    tracing::error!("Failed to insert client");
                }
            }
        })?;

    Ok(name)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger.
    tracing_subscriber::fmt().init();

    run_backend_auto()?;

    Ok(())
}
