use wayland_protocols::wp::presentation_time::server::{wp_presentation, wp_presentation_feedback};
use wayland_server::{Dispatch, DisplayHandle, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<wp_presentation::WpPresentation, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<wp_presentation::WpPresentation>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let presentation = data_init.init(resource, ClientState);
        // Send clock ID (CLOCK_MONOTONIC is usually 1 on Linux)
        presentation.clock_id(1);
    }
}

impl Dispatch<wp_presentation::WpPresentation, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &wp_presentation::WpPresentation,
        request: wp_presentation::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wp_presentation::Request::Feedback { surface, callback } => {
                let feedback = data_init.init(callback, ClientState);
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

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &wp_presentation_feedback::WpPresentationFeedback,
        _request: wp_presentation_feedback::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
    }
}
