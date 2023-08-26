use std::time::Duration;

use smithay::{
    backend::{
        input::InputEvent,
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
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        wayland_server::DisplayHandle,
    },
    utils::{Rectangle, Transform},
};

use crate::{data::Data, state::State};
use smithay::backend::winit;

pub fn init_winit_backend(
    event_loop: &mut EventLoop<Data>,
    dh: &DisplayHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;

    let size = backend.window_size().physical_size;
    let mode = output::Mode {
        size,
        // Aka 60Hz.
        refresh: 60_000,
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
    output.create_global::<State>(dh);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    // Set the preferred mode of the output.
    output.set_preferred(mode);

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
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => match event {
                    InputEvent::Keyboard { event } => {
                        let _ = event;
                        tracing::info!("Keyboard event recieved");
                    }
                    _ => (),
                },
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

    Ok(())
}
