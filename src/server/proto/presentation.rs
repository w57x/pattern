use crate::server::Composer;
use wayland_protocols::wp::presentation_time::server::{wp_presentation, wp_presentation_feedback};
use wayland_server::{Dispatch, DisplayHandle, GlobalDispatch, Resource};

impl GlobalDispatch<wp_presentation::WpPresentation, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<wp_presentation::WpPresentation>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let presentation = data_init.init(resource, ());
        // Send clock ID (CLOCK_MONOTONIC is usually 1 on Linux)
        presentation.clock_id(1);
    }
}

impl Dispatch<wp_presentation::WpPresentation, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &wp_presentation::WpPresentation,
        request: wp_presentation::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_presentation::Request::Feedback { surface, callback } => {
                let feedback = data_init.init(callback, ());
                state
                    .pending_presentation_feedbacks
                    .entry(surface.id())
                    .or_default()
                    .push(feedback);
            }
            wp_presentation::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, ()> for Composer {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &wp_presentation_feedback::WpPresentationFeedback,
        _request: wp_presentation_feedback::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}
