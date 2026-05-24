use crate::config::keybinds::{KeyPattern, KeySpec};
use crate::config::{CompositorCommand, StoredAction};
use crate::{input::Mods, server::Composer};
use std::process::Command;
use wayland_server::Resource;
use wayland_server::protocol::wl_keyboard::KeyState;
use xkbcommon::xkb::Keysym;

pub enum BindingAction {
    Handled,
    Exit,
    None,
}

pub fn execute_compositor_command(
    state: &mut Composer,
    dh: &wayland_server::DisplayHandle,
    cmd: &CompositorCommand,
) -> BindingAction {
    match cmd {
        CompositorCommand::Quit => {
            return BindingAction::Exit;
        }
        CompositorCommand::Exec { full_sh_cmd } => {
            if let Ok(_) = Command::new("sh").args(&["-c", full_sh_cmd]).spawn() {
                return BindingAction::Handled;
            }
        }
        CompositorCommand::CloseWindow { id } => {
            if let Some(win_id) = id {
                if let Some(window) = state
                    .wm
                    .all_windows()
                    .into_iter()
                    .find(|w| w.surface.id().protocol_id() == *win_id)
                {
                    if let Some(toplevel) = &window.toplevel {
                        toplevel.close();
                    }
                }
            } else {
                state.request_closing_active_client();
            }
            return BindingAction::Handled;
        }
        CompositorCommand::FullscreenWindow { id, toggle, value } => {
            state.request_fullscreen_window(*id, *toggle, *value);
            return BindingAction::Handled;
        }
        CompositorCommand::FocusWorkspace { id, next, previous } => {
            let success = if *next {
                state.wm.focus_after_workspace()
            } else if *previous {
                state.wm.focus_before_workspace()
            } else if let Some(ws_id) = id {
                state.wm.focus_workspace(*ws_id)
            } else {
                false
            };

            if success {
                state.needs_redraw = true;
                state.set_input_focus(state.wm.get_focused_window(), dh);
                state.update_pointer_focus(0);
            }
            return BindingAction::Handled;
        }
        CompositorCommand::MoveWindowToWorkspace { id, workspace } => {
            let target_id = if let Some(win_id) = id {
                state
                    .wm
                    .all_windows()
                    .into_iter()
                    .find(|w| w.surface.id().protocol_id() == *win_id)
                    .map(|w| w.surface.id())
            } else {
                state.wm.get_focused_window().map(|s| s.id())
            };

            if let Some(surface_id) = target_id {
                state
                    .wm
                    .move_window_to_workspace(&surface_id, 0, *workspace);
                state.needs_redraw = true;
                state.set_input_focus(state.wm.get_focused_window(), dh);
                state.update_pointer_focus(0);
            }
            return BindingAction::Handled;
        }
        CompositorCommand::DragWindow | CompositorCommand::ResizeWindow => {
            return BindingAction::Handled;
        }
    }
    BindingAction::None
}

pub fn handle_keybinding(
    state: &mut Composer,
    dh: &wayland_server::DisplayHandle,
    key: u32,
    key_state: KeyState,
    keysym: Keysym,
    mods: Mods,
) -> BindingAction {
    if key_state != KeyState::Pressed {
        return BindingAction::None;
    }

    let key_pattern_keysym = KeyPattern {
        mods,
        key: KeySpec::Keysym(keysym.raw()),
    };
    let key_pattern_keycode = KeyPattern {
        mods,
        key: KeySpec::Keycode(key + 8),
    };

    let matched_action = {
        let store = state.config_manager.bindings_store.lock().unwrap();
        store
            .get(&key_pattern_keysym)
            .or_else(|| store.get(&key_pattern_keycode))
            .cloned()
    };

    if let Some(action) = matched_action {
        match &*action {
            StoredAction::Builtin(cmd) => {
                return execute_compositor_command(state, dh, cmd);
            }
            StoredAction::LuaCallback(reg_key) => {
                let res = state
                    .config_manager
                    .ctxt
                    .registry_value::<mlua::Function>(reg_key)
                    .and_then(|func| func.call::<()>(()));
                if let Err(e) = res {
                    tracing::error!("Error in Lua keybinding callback: {:?}", e);
                }
                return BindingAction::Handled;
            }
        }
    }

    BindingAction::None
}
