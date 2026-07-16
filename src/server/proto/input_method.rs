use crate::server::Composer;
use std::os::fd::AsFd;
use wayland_protocols_misc::zwp_input_method_v2::server::{
    zwp_input_method_keyboard_grab_v2::{self, ZwpInputMethodKeyboardGrabV2},
    zwp_input_method_manager_v2::{self, ZwpInputMethodManagerV2},
    zwp_input_method_v2::{self, ZwpInputMethodV2},
    zwp_input_popup_surface_v2::{self, ZwpInputPopupSurfaceV2},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<ZwpInputMethodManagerV2, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpInputMethodManagerV2>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for Composer {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZwpInputMethodManagerV2,
        request: <ZwpInputMethodManagerV2 as Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_input_method_manager_v2::Request::GetInputMethod { input_method, seat } => {
                let input_method = data_init.init(input_method, ());
                _state.input_methods.push((input_method, seat));
            }
            zwp_input_method_manager_v2::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &ZwpInputMethodV2,
        request: <ZwpInputMethodV2 as Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        // Find the focused text_input
        // NOTE: For single-seat, we just find the currently focused text input in the composer.
        let target = state.text_inputs.iter_mut().find(|(_, _, ti_state)| {
            // Check if this text input's surface is currently focused
            if let Some(focus) = &state.input_focus {
                ti_state.surface.as_ref().map(|s| s.id()) == Some(focus.id())
            } else {
                false
            }
        });

        let target_ti = target.as_ref().map(|(ti, _, _)| ti.clone());
        let mut current_serial = 0;
        if let Some((_, _, ti_state)) = target {
            current_serial = ti_state.serial;
        }

        match request {
            zwp_input_method_v2::Request::CommitString { text } => {
                if let Some(ti) = target_ti {
                    ti.commit_string(Some(text));
                }
                if let Some((_, _, ti_state)) = target {
                    ti_state.current_preedit = None;
                }
            }
            zwp_input_method_v2::Request::SetPreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                if let Some(ti) = target_ti {
                    if text.is_empty() {
                        ti.preedit_string(None, 0, 0);
                    } else {
                        ti.preedit_string(Some(text.clone()), cursor_begin, cursor_end);
                    }
                }
                if let Some((_, _, ti_state)) = target {
                    if text.is_empty() {
                        ti_state.current_preedit = None;
                    } else {
                        ti_state.current_preedit = Some((text, cursor_begin, cursor_end));
                    }
                }
            }
            zwp_input_method_v2::Request::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                if let Some(ti) = target_ti {
                    ti.delete_surrounding_text(before_length, after_length);
                }
            }
            zwp_input_method_v2::Request::Commit { serial: _ } => {
                if let Some(ti) = target_ti {
                    ti.done(current_serial);
                }
            }
            zwp_input_method_v2::Request::GetInputPopupSurface { id, surface } => {
                let popup = data_init.init(id, ());

                // Immediately send the text input rectangle if available
                if let Some(ti_state) = target.as_ref().map(|(_, _, state)| state) {
                    let (x, y, w, h) = ti_state.cursor_rect;
                    popup.text_input_rectangle(x, y, w, h);
                }

                state.input_popups.push((popup, surface, resource.clone()));
            }
            zwp_input_method_v2::Request::GrabKeyboard { keyboard } => {
                let grab = data_init.init(keyboard, ());

                // Send initial state to the grab
                grab.keymap(
                    wayland_server::protocol::wl_keyboard::KeymapFormat::XkbV1,
                    state.keymap_fd.as_fd(),
                    state.keymap_size,
                );
                grab.repeat_info(35, 300);

                state.input_method_grabs.push((grab, resource.clone()));
            }
            zwp_input_method_v2::Request::Destroy => {
                state.input_methods.retain(|(im, _)| im != resource);
                state.input_popups.retain(|(_, _, im)| im != resource);
                state.input_method_grabs.retain(|(_, im)| im != resource);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputPopupSurfaceV2, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &ZwpInputPopupSurfaceV2,
        request: <ZwpInputPopupSurfaceV2 as Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let zwp_input_popup_surface_v2::Request::Destroy = request {
            state.input_popups.retain(|(p, _, _)| p != resource);
        }
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &ZwpInputMethodKeyboardGrabV2,
        request: <ZwpInputMethodKeyboardGrabV2 as Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let zwp_input_method_keyboard_grab_v2::Request::Release = request {
            state.input_method_grabs.retain(|(g, _)| g != resource);
        }
    }
}
