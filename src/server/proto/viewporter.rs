use crate::server::Composer;
use wayland_protocols::wp::viewporter::server::{
    wp_viewport, wp_viewport::WpViewport, wp_viewporter, wp_viewporter::WpViewporter,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WpViewporter, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpViewporter>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpViewporter, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WpViewporter,
        request: wp_viewporter::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_viewporter::Request::GetViewport { id, surface } => {
                let viewport = data_init.init(id, ());
                state
                    .surface_to_viewport
                    .insert(surface.id(), viewport.id());
            }
            wp_viewporter::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<WpViewport, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WpViewport,
        request: wp_viewport::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let entry = state.viewports.entry(resource.id()).or_insert((None, None));
        match request {
            wp_viewport::Request::SetSource {
                x,
                y,
                width,
                height,
            } => {
                if x == -1.0 {
                    entry.0 = None;
                } else {
                    entry.0 = Some((x, y, width, height));
                }
            }
            wp_viewport::Request::SetDestination { width, height } => {
                if width == -1 {
                    entry.1 = None;
                } else {
                    entry.1 = Some((width, height));
                }
            }
            wp_viewport::Request::Destroy => {
                state.viewports.remove(&resource.id());
            }
            _ => {}
        }
    }
}
