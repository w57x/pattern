use crate::server::ServerState;
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
    _key: u32,
    key_state: KeyState,
    keysym: Keysym,
) -> BindingAction {
    let super_mod = state.xkb_state.mod_name_is_active(
        &xkbcommon::xkb::MOD_NAME_LOGO,
        xkbcommon::xkb::STATE_MODS_EFFECTIVE,
    );

    if key_state == KeyState::Pressed && super_mod {
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
                if let Ok(_c) = Command::new("kitty").spawn() {
                    return BindingAction::Handled;
                }
            }
            _ => {}
        }
    }

    BindingAction::None
}
