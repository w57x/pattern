use wayland_protocols::wp::text_input::zv3::server::{
    zwp_text_input_manager_v3::{self, ZwpTextInputManagerV3},
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{Dispatch, GlobalDispatch};

use crate::server::{ClientState, Composer, GlobalState};

#[derive(Default)]
pub struct TextInputState {
    pub surface: Option<WlSurface>,

    // Pending state (buffered until commit)
    pub pending_enabled: Option<bool>,
    pub pending_surrounding_text: Option<(String, i32, i32)>,
    pub pending_cause: Option<zwp_text_input_v3::ChangeCause>,
    pub pending_content_type: Option<(
        zwp_text_input_v3::ContentHint,
        zwp_text_input_v3::ContentPurpose,
    )>,
    pub pending_cursor_rect: Option<(i32, i32, i32, i32)>,

    // Current state (active after commit)
    pub active: bool,
    pub cursor_rect: (i32, i32, i32, i32),
    pub serial: u32,
    pub current_preedit: Option<(String, i32, i32)>,
}

impl GlobalDispatch<ZwpTextInputManagerV3, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpTextInputManagerV3>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwpTextInputManagerV3, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwpTextInputManagerV3,
        request: <ZwpTextInputManagerV3 as wayland_server::Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_text_input_manager_v3::Request::Destroy => {}
            zwp_text_input_manager_v3::Request::GetTextInput { id, seat } => {
                let text_input = data_init.init(id, ClientState);
                state
                    .text_inputs
                    .push((text_input, seat, TextInputState::default()));
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputV3, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwpTextInputV3,
        request: <ZwpTextInputV3 as wayland_server::Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let text_input_data = state
            .text_inputs
            .iter_mut()
            .find(|(ti, _, _)| ti == resource);

        let (_, _seat, ti_state) = match text_input_data {
            Some(data) => data,
            None => return,
        };

        match request {
            zwp_text_input_v3::Request::Destroy => {
                state.text_inputs.retain(|(ti, _, _)| ti != resource);
            }
            zwp_text_input_v3::Request::Enable => {
                ti_state.pending_enabled = Some(true);
            }
            zwp_text_input_v3::Request::Disable => {
                ti_state.pending_enabled = Some(false);
            }
            zwp_text_input_v3::Request::SetSurroundingText {
                text,
                cursor,
                anchor,
            } => {
                ti_state.pending_surrounding_text = Some((text, cursor, anchor));
            }
            zwp_text_input_v3::Request::SetTextChangeCause { cause } => {
                ti_state.pending_cause = Some(cause);
            }
            zwp_text_input_v3::Request::SetContentType { hint, purpose } => {
                ti_state.pending_content_type = Some((hint, purpose));
            }
            zwp_text_input_v3::Request::SetCursorRectangle {
                x,
                y,
                width,
                height,
            } => {
                ti_state.pending_cursor_rect = Some((x, y, width, height));
            }
            zwp_text_input_v3::Request::Commit => {
                // Apply pending state
                if let Some(enabled) = ti_state.pending_enabled.take() {
                    ti_state.active = enabled;
                }
                if let Some(rect) = ti_state.pending_cursor_rect.take() {
                    ti_state.cursor_rect = rect;
                }

                // Forward to IME
                // NOTE: In a multi-seat system, we would compare seat names/identities.
                // For Pattern (single-seat), we find the first available input method for the client's seat.
                let im_data = state.input_methods.first();

                if let Some((im, _)) = im_data {
                    if ti_state.active {
                        im.activate();
                        if let Some((text, cursor, anchor)) = &ti_state.pending_surrounding_text {
                            im.surrounding_text(
                                text.clone(),
                                (*cursor).try_into().unwrap_or(0),
                                (*anchor).try_into().unwrap_or(0),
                            );
                        }
                        if let Some(cause) = &ti_state.pending_cause {
                            im.text_change_cause(*cause);
                        }
                        if let Some((hint, purpose)) = &ti_state.pending_content_type {
                            im.content_type(*hint, *purpose);
                        }
                        let (x, y, w, h) = ti_state.cursor_rect;
                        for (popup, _, popup_im) in &state.input_popups {
                            if popup_im == im {
                                popup.text_input_rectangle(x, y, w, h);
                            }
                        }
                        im.done();
                    } else {
                        im.deactivate();
                        im.done();
                        ti_state.current_preedit = None;
                    }
                }

                // Clear pending state after forwarding
                ti_state.pending_surrounding_text = None;
                ti_state.pending_cause = None;
                ti_state.pending_content_type = None;

                // Track the commit serial for when we need to send events back
                ti_state.serial += 1;
            }
            zwp_text_input_v3::Request::SetAvailableActions { .. } => {
                // Not strictly necessary for basic implementation
            }
            zwp_text_input_v3::Request::ShowInputPanel => {
                // Hint to show virtual keyboard
            }
            zwp_text_input_v3::Request::HideInputPanel => {
                // Hint to hide virtual keyboard
            }
            _ => {}
        }
    }
}
