use std::collections::HashSet;

use crate::{backend::Error, state::State};
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
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement, Bind,
            ImportAll, Renderer,
        },
    },
    desktop::{space::render_output, Space, Window},
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_server::DisplayHandle,
    utils::Transform,
};
use smithay_drm_extras::edid::EdidInfo;

use super::DrmData;

pub struct OutputSurface {
    pub gbm_surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    pub output: Output,
    pub damage_tracked_renderer: OutputDamageTracker,
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
        })
    }

    pub fn next_buffer<R>(&mut self, space: &Space<Window>, renderer: &mut R)
    where
        R: Renderer + ImportAll + Bind<Dmabuf>,
        R::TextureId: 'static,
    {
        let dmabuf = self.gbm_surface.next_buffer().unwrap().0;
        renderer.bind(dmabuf).unwrap();

        let res = render_output::<_, WaylandSurfaceRenderElement<R>, _, _>(
            &self.output,
            renderer,
            1.0,
            0,
            [space],
            &[],
            &mut self.damage_tracked_renderer,
            [0.1, 0.1, 0.1, 1.0],
        )
        .unwrap();

        self.gbm_surface.queue_buffer(None, res.damage, ()).ok();
    }
}
