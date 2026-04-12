use crate::server::definition::ServerState;
use std::process::Command;
use wayland_server::protocol::wl_keyboard::KeyState;
use xkbcommon::xkb::Keysym;

pub enum BindingAction {
    Handled,
    Exit,
    None,
}

pub fn handle_keybinding(
    state: &mut ServerState,
    key: u32,
    key_state: KeyState,
    keysym: Keysym,
) -> BindingAction {
    if key == 125 || key == 126 {
        state.super_held = key_state == KeyState::Pressed;
        return BindingAction::Handled;
    }

    if key_state == KeyState::Pressed && state.super_held {
        match keysym.raw() {
            xkbcommon::xkb::keysyms::KEY_q => {
                if let Some(active_window) = state.windows.last() {
                    active_window.close();
                }
                return BindingAction::Handled;
            }
            xkbcommon::xkb::keysyms::KEY_e => {
                return BindingAction::Exit;
            }
            xkbcommon::xkb::keysyms::KEY_t => {
                Command::new("kitty").spawn().ok();
                return BindingAction::Handled;
            }
            _ => {}
        }
    }

    BindingAction::None
}
