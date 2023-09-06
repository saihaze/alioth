use drm_fourcc::DrmFourcc;
use smithay::{
    backend::drm::{DrmEvent, DrmNode},
    reexports::wayland_server::DisplayHandle,
};
use smithay_drm_extras::drm_scanner::DrmScanEvent;

use crate::{
    backend::drm::{surface::OutputSurface, DrmData},
    state::State,
};

const SUPPORTED_FORMATS: &[DrmFourcc] = &[
    DrmFourcc::Abgr2101010,
    DrmFourcc::Argb2101010,
    DrmFourcc::Abgr8888,
    DrmFourcc::Argb8888,
];

impl State<DrmData> {
    pub fn on_drm_event(&mut self, node: DrmNode, event: DrmEvent) {
        match event {
            DrmEvent::VBlank(crtc) => {
                if let Some(device) = self.backend_data.devices.get_mut(&node) {
                    if let Some(surface) = device.surfaces.get_mut(&crtc) {
                        let mut renderer = if self.backend_data.primary_gpu == device.render_node {
                            self.backend_data
                                .gpu_manager
                                .single_renderer(&device.render_node)
                                .unwrap()
                        } else {
                            self.backend_data
                                .gpu_manager
                                .renderer(
                                    &self.backend_data.primary_gpu,
                                    &device.render_node,
                                    &mut device.gbm_allocator,
                                    surface.gbm_surface.format(),
                                )
                                .unwrap()
                        };
                        surface.gbm_surface.frame_submitted().unwrap();
                        surface.next_buffer(&self.space, &mut renderer);
                    }
                }
            }
            _ => (),
        }
    }

    pub fn on_drm_connector_event(
        &mut self,
        dh: &DisplayHandle,
        node: DrmNode,
        event: DrmScanEvent,
    ) {
        let device = if let Some(device) = self.backend_data.devices.get_mut(&node) {
            device
        } else {
            return;
        };

        match event {
            DrmScanEvent::Connected {
                connector,
                crtc: Some(crtc),
            } => {
                let mut renderer = self
                    .backend_data
                    .gpu_manager
                    .single_renderer(&device.render_node)
                    .unwrap();

                let mut surface = OutputSurface::new(
                    dh,
                    crtc,
                    &connector,
                    SUPPORTED_FORMATS,
                    renderer
                        .as_mut()
                        .egl_context()
                        .dmabuf_render_formats()
                        .clone(),
                    &device.drm,
                    device.gbm.clone(),
                )
                .unwrap();
                let output = surface.output.clone();
                surface.next_buffer(&self.space, &mut renderer);
                device.surfaces.insert(crtc, surface);
                self.map_output_on_the_right(output);
            }
            DrmScanEvent::Disconnected {
                crtc: Some(crtc), ..
            } => {
                device.surfaces.remove(&crtc);
            }
            _ => (),
        }
    }
}
