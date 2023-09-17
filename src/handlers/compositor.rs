use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor,
    desktop::PopupKind,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Client},
    wayland::{
        compositor::{
            get_parent, is_sync_subsurface, with_states, CompositorClientState, CompositorHandler,
            CompositorState,
        },
        shell::xdg::{XdgPopupSurfaceData, XdgToplevelSurfaceData},
    },
};

use crate::{data::ClientData, grabs::resize_grab, state::State};

impl<BackendData> CompositorHandler for State<BackendData> {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientData>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == &root)
            {
                window.on_commit();
            }
        }

        if let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().wl_surface() == surface)
            .cloned()
        {
            let initial_configure_sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            });

            if !initial_configure_sent {
                window.toplevel().send_configure();
            }
        } else if let Some(popup) = self.popups.find_popup(surface) {
            let PopupKind::Xdg(ref popup) = popup;
            let initial_configure_sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgPopupSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            });
            if !initial_configure_sent {
                popup.send_configure().unwrap();
            }
        }

        resize_grab::handle_commit(&mut self.space, surface);
    }
}
delegate_compositor!(@<BackendData: 'static> State<BackendData>);
