use backend::init_backend_auto;
use data::{ClientData, Data};
use smithay::{
    desktop::Space,
    input::SeatState,
    reexports::{
        calloop::{self, generic::Generic, EventLoop, Interest, PostAction},
        wayland_server::Display,
    },
    wayland::{
        compositor::CompositorState, data_device::DataDeviceState, output::OutputManagerState,
        shell::xdg::XdgShellState, shm::ShmState, socket::ListeningSocketSource,
    },
};
use state::State;
use std::{os::fd::AsRawFd, sync::Arc, time::Instant};

mod backend;
mod data;
mod handlers;
mod input;
mod state;
mod workspace;

/// Create a Unix socket for the Wayland server.
fn init_wayland_socket(event_loop: &mut EventLoop<Data>) -> Result<String, anyhow::Error> {
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
                    tracing::error!("Failed to insert client")
                }
            }
        })?;

    Ok(name)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger.
    tracing_subscriber::fmt::init();

    // Create an event loop.
    let mut event_loop = EventLoop::try_new()?;
    // Create a Wayland display.
    // Displays are all about the Wayland protocol and do no rendering.
    let mut display = Display::new()?;

    // Create a Unix socket for clients to connect to.
    let socket = init_wayland_socket(&mut event_loop)?;

    // Insert the display to the event loop.
    // In wlroots, we directly use wl_display's event loop. But now we add it to our own one.
    event_loop.handle().insert_source(
        Generic::new(
            display.backend().poll_fd().as_raw_fd(),
            Interest::READ,
            calloop::Mode::Level,
        ),
        |_, _, data| {
            // Handle the events from the display, once.
            data.display.dispatch_clients(&mut data.state).unwrap();
            // Then we continue listening for other events.
            Ok(PostAction::Continue)
        },
    )?;

    // Get the display handle. Again: it is just related to the Wayland protocol and has nothing to
    // do with the backend.
    let dh = display.handle();

    // Used to compose.
    let compositor_state = CompositorState::new::<State>(&dh);
    // Used to create shared memory buffers.
    let shm_state = ShmState::new::<State>(&dh, vec![]);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<State>(&dh);
    let xdg_shell_state = XdgShellState::new::<State>(&dh);
    let mut seat_state = SeatState::new();
    let data_device_state = DataDeviceState::new::<State>(&dh);

    let mut seat = seat_state.new_wl_seat(&dh, "alioth");
    // FIXME: Implement hot-plug
    seat.add_keyboard(Default::default(), 200, 200)?;
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
    };
    // Pack the event loop data.
    let mut data = Data { state, display };

    // Initialize backend. The proper backend will be selected automatically.
    init_backend_auto(&mut event_loop, &dh, &mut data.state)?;

    // Set the WAYLAND_DISPLAY environment variable.
    std::env::set_var("WAYLAND_DISPLAY", socket);

    // Run the event loop. It blocks.
    event_loop.run(None, &mut data, |_| {})?;

    Ok(())
}
