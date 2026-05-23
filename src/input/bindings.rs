use crate::{input::Mods, server::Composer};
use std::process::Command;
use wayland_server::protocol::wl_keyboard::KeyState;
use xkbcommon::xkb::{Keysym, keysyms};

pub enum BindingAction {
    Handled,
    Exit,
    None,
}

pub fn handle_keybinding(
    state: &mut Composer,
    dh: &wayland_server::DisplayHandle,
    _key: u32,
    key_state: KeyState,
    keysym: Keysym,
    mods: Mods,
) -> BindingAction {
    if key_state == KeyState::Pressed && mods.mod4 {
        match keysym.raw() {
            keysyms::KEY_q => {
                state.request_closing_active_client();
                return BindingAction::Handled;
            }

            keysyms::KEY_e => {
                return BindingAction::Exit;
            }

            keysyms::KEY_t => {
                if let Ok(_c) = Command::new("kitty").spawn() {
                    return BindingAction::Handled;
                }
            }

            keysyms::KEY_s => {
                if let Ok(_c) = Command::new("seekr").spawn() {
                    return BindingAction::Handled;
                }
            }

            keysyms::KEY_Right => {
                if mods.alt {
                    if state.wm.focus_after_workspace() {
                        state.needs_redraw = true;
                        state.set_input_focus(state.wm.get_focused_window(), dh);
                        state.update_pointer_focus(0);
                    }
                    return BindingAction::Handled;
                }
            }

            keysyms::KEY_Left => {
                if mods.alt {
                    if state.wm.focus_before_workspace() {
                        state.needs_redraw = true;
                        state.set_input_focus(state.wm.get_focused_window(), dh);
                        state.update_pointer_focus(0);
                    }
                    return BindingAction::Handled;
                }
            }
            _ => {}
        }
    }

    BindingAction::None
}
