use wayland_protocols_misc::zwp_virtual_keyboard_v1::server::{
    zwp_virtual_keyboard_manager_v1::{self, ZwpVirtualKeyboardManagerV1},
    zwp_virtual_keyboard_v1::{self, ZwpVirtualKeyboardV1},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<ZwpVirtualKeyboardManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpVirtualKeyboardManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwpVirtualKeyboardManagerV1,
        request: <ZwpVirtualKeyboardManagerV1 as Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let zwp_virtual_keyboard_manager_v1::Request::CreateVirtualKeyboard { seat: _, id } =
            request
        {
            let _vk = data_init.init(id, ClientState);
            // We could track virtual keyboards in state, but for now we just handle their requests
        }
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwpVirtualKeyboardV1,
        request: <ZwpVirtualKeyboardV1 as Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_virtual_keyboard_v1::Request::Keymap {
                format: _,
                fd: _,
                size: _,
            } => {
                // Fcitx5 provides a keymap to the compositor
            }
            zwp_virtual_keyboard_v1::Request::Key {
                time,
                key,
                state: key_state,
            } => {
                // Fcitx5 emits a key event. We should inject this into the currently focused surface.
                let target_client = state.input_focus.as_ref().and_then(|f| f.client());
                if let Some(client) = target_client {
                    let serial = state.serial;
                    state.serial += 1;

                    let key_state_enum = if key_state == 0 {
                        wayland_server::protocol::wl_keyboard::KeyState::Released
                    } else {
                        wayland_server::protocol::wl_keyboard::KeyState::Pressed
                    };

                    for keyboard in state
                        .keyboards
                        .iter()
                        .filter(|k| k.client().map(|c| c.id()) == Some(client.id()))
                    {
                        keyboard.key(serial, time, key, key_state_enum);
                    }
                }
            }
            zwp_virtual_keyboard_v1::Request::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                // Update modifiers for the currently focused surface
                let target_client = state.input_focus.as_ref().and_then(|f| f.client());
                if let Some(client) = target_client {
                    let serial = state.serial;
                    state.serial += 1;
                    for keyboard in state
                        .keyboards
                        .iter()
                        .filter(|k| k.client().map(|c| c.id()) == Some(client.id()))
                    {
                        keyboard.modifiers(
                            serial,
                            mods_depressed,
                            mods_latched,
                            mods_locked,
                            group,
                        );
                    }
                }
            }
            zwp_virtual_keyboard_v1::Request::Destroy => {}
            _ => {}
        }
    }
}
