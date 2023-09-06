use std::{os::fd::FromRawFd, path::PathBuf};

use crate::{
    backend::drm::{Device, DrmData},
    state::State,
};
use smithay::{
    backend::{
        allocator::{
            dmabuf::DmabufAllocator,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd, DrmNode},
        egl::{EGLDevice, EGLDisplay},
        session::Session,
        udev::UdevEvent,
    },
    reexports::{nix::fcntl::OFlag, wayland_server::DisplayHandle},
    utils::DeviceFd,
};
use smithay_drm_extras::drm_scanner::DrmScanner;

impl State<DrmData> {
    pub fn on_udev_event(&mut self, dh: &DisplayHandle, event: UdevEvent) {
        match event {
            UdevEvent::Added { device_id, path } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    self.on_device_added(dh, node, path);
                }
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    self.on_device_changed(dh, node);
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    self.on_device_removed(node);
                }
            }
        }
    }

    fn on_device_added(&mut self, dh: &DisplayHandle, node: DrmNode, path: PathBuf) {
        let fd = self
            .backend_data
            .session
            .open(
                &path,
                OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
            )
            .unwrap();
        let fd = DrmDeviceFd::new(unsafe { DeviceFd::from_raw_fd(fd) });

        let (drm, drm_notifier) = DrmDevice::new(fd, false).unwrap();

        let gbm = GbmDevice::new(drm.device_fd().clone()).unwrap();
        let gbm_allocator = GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING);

        let render_node =
            match EGLDevice::device_for_display(&EGLDisplay::new(gbm.clone()).unwrap())
                .ok()
                .and_then(|x| x.try_get_render_node().ok().flatten())
            {
                Some(node) => node,
                None => node,
            };

        self.backend_data
            .gpu_manager
            .as_mut()
            .add_node(render_node, gbm.clone())
            .unwrap();

        self.backend_data
            .event_loop_handle
            .insert_source(drm_notifier, move |event, _, data| {
                data.state.on_drm_event(node, event);
            })
            .unwrap();

        self.backend_data.devices.insert(
            node,
            Device {
                drm,
                gbm,
                gbm_allocator: DmabufAllocator(gbm_allocator),
                surfaces: Default::default(),
                render_node,
                drm_scanner: DrmScanner::new(),
            },
        );

        self.on_device_changed(dh, node);
    }

    fn on_device_changed(&mut self, dh: &DisplayHandle, node: DrmNode) {
        if let Some(device) = self.backend_data.devices.get_mut(&node) {
            for event in device.drm_scanner.scan_connectors(&device.drm) {
                self.on_drm_connector_event(dh, node, event);
            }
        }
    }

    fn on_device_removed(&mut self, node: DrmNode) {
        if let Some(device) = self.backend_data.devices.get_mut(&node) {
            self.backend_data
                .gpu_manager
                .as_mut()
                .remove_node(&device.render_node);
        }
    }
}
