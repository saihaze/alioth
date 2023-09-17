use std::{collections::HashSet, time::Duration};

use crate::{
    backend::Error,
    cursor::{CursorElement, PointerRenderElement},
    state::State,
};
use drm::control::{connector, crtc, ModeTypeFlags};
use drm_fourcc::{DrmFormat, DrmFourcc};

use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd, GbmBufferedSurface},
        renderer::{
            damage::OutputDamageTracker, element::AsRenderElements, Bind, ImportAll, ImportMem,
            Renderer,
        },
    },
    desktop::{space::render_output, utils::surface_primary_scanout_output, Space, Window},
    input::pointer::{CursorImageStatus, PointerHandle},
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_server::DisplayHandle,
    utils::{Clock, Monotonic, Transform},
    wayland::compositor::{self, SurfaceData},
};
use smithay_drm_extras::edid::EdidInfo;
use std::time::Instant;

use super::DrmData;

pub struct OutputSurface {
    pub gbm_surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    pub output: Output,
    pub damage_tracked_renderer: OutputDamageTracker,
    pub cursor: CursorElement,
}

impl OutputSurface {
    pub fn new(
        dh: &DisplayHandle,
        crtc: crtc::Handle,
        connector: &connector::Info,
        color_formats: &[DrmFourcc],
        renderer_formats: HashSet<DrmFormat>,
        drm: &DrmDevice,
        gbm: GbmDevice<DrmDeviceFd>,
    ) -> Result<Self, Error> {
        // Find the position of the preferred mode.
        // If no mode is marked with PREFERRED, the first (index 0) will be selected.
        let mode_id = connector
            .modes()
            .iter()
            .position(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(0);

        let preferred_mode = connector.modes()[mode_id];

        let drm_surface = drm
            .create_surface(crtc, preferred_mode, &[connector.handle()])
            .or_else(|_| {
                tracing::error!("Failed to create drm surface");
                Err(Error::DrmSurfaceCreateFailure)
            })?;

        let gbm_surface = GbmBufferedSurface::new(
            drm_surface,
            GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT),
            color_formats,
            renderer_formats,
        )
        .or_else(|_| {
            tracing::error!("Failed to create gbm surface");
            Err(Error::GbmSurfaceCreateFailure)
        })?;

        let name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );

        let (make, model) = EdidInfo::for_connector(drm, connector.handle())
            .map(|info| (info.manufacturer, info.model))
            .unwrap_or(("Unknown".into(), "Unknown".into()));

        let (width, height) = connector.size().unwrap_or((0, 0));
        let output = Output::new(
            name,
            PhysicalProperties {
                size: (width as i32, height as i32).into(),
                subpixel: Subpixel::Unknown,
                make,
                model,
            },
        );

        let output_mode = Mode::from(preferred_mode);
        output.set_preferred(output_mode);
        output.create_global::<State<DrmData>>(&dh);
        output.change_current_state(
            Some(output_mode),
            Some(Transform::Normal),
            // TODO: Scale will be set here.
            Some(Scale::Integer(1)),
            None,
        );

        let damage_tracked_renderer = OutputDamageTracker::from_output(&output);

        Ok(Self {
            gbm_surface,
            output,
            damage_tracked_renderer,
            cursor: CursorElement::new().map_err(|err| Error::CursorLoadError(err))?,
        })
    }

    pub fn next_buffer<R>(
        &mut self,
        space: &Space<Window>,
        start_time: Instant,
        renderer: &mut R,
        pointer: Option<&PointerHandle<State<DrmData>>>,
        clock: &Clock<Monotonic>,
        cursor_status: CursorImageStatus,
    ) where
        R: Renderer + ImportAll + ImportMem + Bind<Dmabuf>,
        R::TextureId: 'static + Clone,
    {
        let dmabuf = self.gbm_surface.next_buffer().unwrap().0;
        renderer.bind(dmabuf).unwrap();

        let cursor_elements = match pointer {
            Some(pointer) => {
                if space
                    .output_under(pointer.current_location())
                    .find(|output| **output == self.output)
                    .is_some()
                {
                    let scale = self.output.current_scale().fractional_scale();

                    self.cursor.update_animation_status(clock);
                    self.cursor.set_status(cursor_status);

                    self.cursor.render_elements::<PointerRenderElement<R>>(
                        renderer,
                        pointer.current_location().to_physical(scale).to_i32_round(),
                        scale.into(),
                        1.0,
                    )
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        };

        let res = render_output::<_, PointerRenderElement<R>, _, _>(
            &self.output,
            renderer,
            1.0,
            0,
            [space],
            cursor_elements.as_slice(),
            &mut self.damage_tracked_renderer,
            [0.1, 0.1, 0.1, 1.0],
        )
        .unwrap();

        for window in space.elements() {
            let output = compositor::with_states(window.toplevel().wl_surface(), |states| {
                let data = states.data_map.get::<SurfaceData>().unwrap();
                surface_primary_scanout_output(window.toplevel().wl_surface(), data)
            });

            if output == Some(self.output.clone()) {
                window.send_frame(
                    &self.output,
                    start_time.elapsed(),
                    Some(Duration::ZERO),
                    |_, _| Some(self.output.clone()),
                );
            }
        }

        self.gbm_surface.queue_buffer(None, res.damage, ()).ok();
    }
}
