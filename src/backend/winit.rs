use std::{os::fd::AsRawFd, time::Duration};

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
            gles::GlesRenderer,
        },
        winit::{WinitError, WinitEvent},
    },
    desktop::space::render_output,
    output,
    reexports::{
        calloop::{
            self,
            generic::Generic,
            timer::{TimeoutAction, Timer},
            EventLoop, Interest, PostAction,
        },
        wayland_server::Display,
    },
    utils::{Rectangle, Transform},
};

use crate::{backend::Error, data::Data, init_wayland_socket, input::Action, state::State};
use smithay::backend::winit;

const REFRESH_RATE: i32 = 60_000;

pub fn run_winit_backend() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::<Data<()>>::try_new()?;
    // Create a Wayland display.
    // Displays are all about the Wayland protocol and do no rendering.
    let mut display = Display::<State<()>>::new().or_else(|_| {
        tracing::error!("Failed to create display");
        Err(Error::DisplayCreateFailure)
    })?;

    // Create a Unix socket for clients to connect to.
    let socket = init_wayland_socket(&mut event_loop).or_else(|_| {
        tracing::error!("Failed to create Wayland socket");
        Err(Error::SocketCreateFailure)
    })?;

    // Insert the display to the event loop.
    // In wlroots, we directly use wl_display's event loop. But now we add it to our own one.
    event_loop
        .handle()
        .insert_source(
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
        )
        .or_else(|_| {
            tracing::error!("Failed to insert the display to the event loop");
            Err(Error::SourceInsertFailure)
        })?;

    let dh = display.handle();
    let mut state =
        State::new(&display, &mut event_loop, ()).map_err(|e| Error::StateCreateFailure(e))?;

    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;

    let size = backend.window_size().physical_size;
    let mode = output::Mode {
        size,
        // Aka 60Hz.
        refresh: REFRESH_RATE,
    };

    // Properties of the output.
    // Winit windows can be resized, so size doesn't need to be correct.
    let physical_properties = output::PhysicalProperties {
        size: (0, 0).into(),
        subpixel: output::Subpixel::Unknown,
        make: "alioth".into(),
        model: "Alioth Winit Output".into(),
    };

    // Create an output.
    let output = output::Output::new("alioth".to_string(), physical_properties);
    // An output is also a global object.
    output.create_global::<State<()>>(&dh);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    // Set the preferred mode of the output.
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    // Dispatch Winit events.
    event_loop
        .handle()
        .insert_source(Timer::immediate(), move |_, _, data| {
            let display = &mut data.display;
            let state = &mut data.state;

            let res = winit.dispatch_new_events(|event| match event {
                WinitEvent::Resized { size, .. } => {
                    output.change_current_state(
                        Some(output::Mode {
                            size,
                            refresh: REFRESH_RATE,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => {
                    let action = state.handle_input(event);
                    match action {
                        Action::ChangeVt(_) | Action::None => (),
                        Action::Quit => {
                            state.loop_signal.stop();
                        }
                    }
                }
                _ => (),
            });
            if let Err(WinitError::WindowClosed) = res {
                state.loop_signal.stop();
                return TimeoutAction::Drop;
            }

            let damage = Rectangle::from_loc_and_size((0, 0), size);

            backend.bind().unwrap();
            render_output::<_, WaylandSurfaceRenderElement<GlesRenderer>, _, _>(
                &output,
                backend.renderer(),
                1.0,
                0,
                [&state.space],
                &[],
                &mut damage_tracker,
                [0.1, 0.1, 0.1, 1.0],
            )
            .unwrap();
            backend.submit(Some(&[damage])).unwrap();

            for window in state.space.elements() {
                window.send_frame(
                    &output,
                    state.start_time.elapsed(),
                    Some(Duration::ZERO),
                    |_, _| Some(output.clone()),
                );
            }
            state.space.refresh();
            display.flush_clients().unwrap();

            TimeoutAction::ToDuration(Duration::from_millis(16))
        })?;

    std::env::set_var("WAYLAND_DISPLAY", &socket);

    // Pack event loop data.
    let mut data = Data { display, state };
    event_loop.run(None, &mut data, |_| {})?;

    Ok(())
}
