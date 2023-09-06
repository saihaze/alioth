mod handlers;
mod surface;

use drm::control::crtc;

use smithay::{
    backend::{
        allocator::{
            dmabuf::DmabufAllocator,
            gbm::{GbmAllocator, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            gles::GlesRenderer,
            multigpu::{gbm::GbmGlesBackend, GpuManager},
        },
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{primary_gpu, UdevBackend, UdevEvent},
    },
    reexports::{
        calloop::{
            self, generic::Generic, timer::Timer, EventLoop, Interest, LoopHandle, PostAction,
        },
        input::Libinput,
        wayland_server::Display,
    },
};
use smithay_drm_extras::drm_scanner::DrmScanner;
use std::os::fd::AsRawFd;
use std::{collections::HashMap, time::Duration};

use crate::{data::Data, init_wayland_socket, state::State};

use self::surface::OutputSurface;

use super::Error;

struct Device {
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    surfaces: HashMap<crtc::Handle, OutputSurface>,
    render_node: DrmNode,
    gbm_allocator: DmabufAllocator<GbmAllocator<DrmDeviceFd>>,
    drm_scanner: DrmScanner,
}

pub struct DrmData {
    event_loop_handle: LoopHandle<'static, Data<Self>>,
    session: LibSeatSession,
    devices: HashMap<DrmNode, Device>,
    primary_gpu: DrmNode,
    gpu_manager: GpuManager<GbmGlesBackend<GlesRenderer>>,
}

pub fn run_drm_backend() -> Result<(), Error> {
    let mut event_loop = EventLoop::<Data<DrmData>>::try_new().or_else(|_| {
        tracing::error!("Failed to create event loop");
        Err(Error::EventLoopCreateFailure)
    })?;

    // Create a Wayland display.
    // Displays are all about the Wayland protocol and do no rendering.
    let mut display = Display::<State<DrmData>>::new().or_else(|_| {
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

    // Initialize session.
    let (session, notifier) = LibSeatSession::new().or_else(|_| {
        tracing::error!("Failed to initialize session");
        Err(Error::SessionInitFailure)
    })?;

    // Initialize the compositor.
    let primary_gpu = {
        let path = match primary_gpu(&session.seat()) {
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
        DrmNode::from_path(&path)
            .and_then(|node| Ok(node.node_with_type(NodeType::Render).unwrap()?))
            .or_else(|_| {
                tracing::error!("No GPU found");
                Err(Error::NoGPUFound)
            })?
    };
    tracing::info!("Using {} as primary GPU", primary_gpu);

    let backend_data = DrmData {
        event_loop_handle: event_loop.handle(),
        session,
        devices: HashMap::default(),
        primary_gpu,
        gpu_manager: GpuManager::new(Default::default()).or_else(|_| {
            tracing::error!("Failed to create GPU manager");
            Err(Error::GpuManagerCreateFailure)
        })?,
    };
    let mut state = State::new(&display, &mut event_loop, backend_data)
        .map_err(|err| Error::StateCreateFailure(err))?;

    let dh = display.handle();

    // Initialize the udev backend.
    let udev_backend = UdevBackend::new(&state.backend_data.session.seat()).or_else(|_| {
        tracing::error!("Failed to initialize Udev backend");
        Err(Error::UdevInitFailure)
    })?;
    for (device_id, path) in udev_backend.device_list() {
        state.on_udev_event(
            &dh,
            UdevEvent::Added {
                device_id,
                path: path.to_owned(),
            },
        );
    }

    // Initialize the libinput backend.
    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        state.backend_data.session.clone().into(),
    );
    libinput_context
        .udev_assign_seat(&state.backend_data.session.seat())
        .or_else(|_| {
            tracing::error!("Failed to assign seat to libinput context");
            Err(Error::LibinputAssignSeatFailure)
        })?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    // Handle input events.
    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, data| {
            match event {
                InputEvent::Keyboard { .. } => {
                    data.state.backend_data.session.change_vt(2).ok();
                }
                _ => (),
            }
            data.state.handle_input(event);
        })
        .map_err(|_| Error::SourceInsertFailure)?;

    // Handle session events.
    event_loop
        .handle()
        .insert_source(notifier, move |event, _, data| match event {
            SessionEvent::PauseSession => {
                tracing::info!("Pausing session");

                libinput_context.suspend();
                for backend in data.state.backend_data.devices.values() {
                    backend.drm.pause();
                }
            }
            SessionEvent::ActivateSession => {
                tracing::info!("Resuming session");

                if libinput_context.resume().is_err() {
                    tracing::error!("Failed to resume session");
                    return;
                }

                let mut renderers = Vec::new();
                for (node, device) in data.state.backend_data.devices.iter_mut() {
                    device.drm.activate();

                    for (crtc, surface) in device.surfaces.iter_mut() {
                        surface.gbm_surface.reset_buffers();

                        renderers.push((*node, *crtc));
                    }
                }

                for (node, crtc) in renderers {
                    data.state.on_drm_event(node, DrmEvent::VBlank(crtc));
                }
            }
        })
        .map_err(|_| Error::SourceInsertFailure)?;

    // Handle udev events.
    event_loop
        .handle()
        .insert_source(udev_backend, |event, _, data| {
            data.state.on_udev_event(&data.display.handle(), event);
        })
        .map_err(|_| Error::SourceInsertFailure)?;

    std::env::set_var("WAYLAND_DISPLAY", &socket);

    event_loop
        .handle()
        .insert_source(Timer::from_duration(Duration::from_secs(30)), |_, _, _| {
            panic!("Aborted");
        })
        .unwrap();

    let mut data = Data { display, state };
    event_loop.run(None, &mut data, |data| {
        data.display.flush_clients().unwrap();
    }).unwrap();

    Ok(())
}
