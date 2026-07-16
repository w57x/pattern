use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::prelude::*;
pub mod keybinds;
use keybinds::{KeyPattern, parse_keybind};

use crate::styler::style::StyleConfig;

#[derive(Clone, Debug)]
pub enum CompositorCommand {
    Quit,
    Exec {
        full_sh_cmd: String,
    },
    CloseWindow {
        id: Option<u32>,
    },
    FullscreenWindow {
        id: Option<u32>,
        toggle: bool,
        value: bool,
    },
    FocusWorkspace {
        id: Option<usize>,
        next: bool,
        previous: bool,
    },
    DragWindow,
    ResizeWindow,
    MoveWindowToWorkspace {
        id: Option<u32>,
        workspace: usize,
    },
}

impl LuaUserData for CompositorCommand {}

pub enum StoredAction {
    Builtin(CompositorCommand),
    LuaCallback(mlua::RegistryKey),
}

#[derive(Clone, Debug)]
pub struct InputConfig {
    pub kb_layout: String,
    pub kb_variant: String,
    pub kb_model: String,
    pub kb_options: String,
    pub kb_rules: String,
    pub repeat_rate: u32,
    pub repeat_delay: u32,
    pub sensitivity: f64,
    pub natural_scroll: bool,
    pub disable_while_typing: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            kb_layout: "us".to_string(),
            kb_variant: "".to_string(),
            kb_model: "".to_string(),
            kb_options: "".to_string(),
            kb_rules: "".to_string(),
            repeat_rate: 25,
            repeat_delay: 600,
            sensitivity: 0.0,
            natural_scroll: false,
            disable_while_typing: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GesturesConfig {
    pub workspace_swipe_invert: bool,
    pub workspace_swipe_threshold: f64,
}

impl Default for GesturesConfig {
    fn default() -> Self {
        Self {
            workspace_swipe_invert: true,
            workspace_swipe_threshold: 300.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GestureConfig {
    pub fingers: u32,
    pub direction: String,
    pub action: String,
}

#[derive(Clone, Debug, Default)]
pub struct CompositorConfig {
    pub input: InputConfig,
    pub gestures: GesturesConfig,
    pub registered_gestures: Vec<GestureConfig>,
    pub style: StyleConfig,
}

type BindingsStore = Arc<Mutex<HashMap<KeyPattern, Arc<StoredAction>>>>;

pub struct ConfigManager {
    pub bindings_store: BindingsStore,
    pub config_dir: PathBuf,
    pub ctxt: Lua,
    pub config: Arc<Mutex<CompositorConfig>>,
    pub hooks: Arc<Mutex<HashMap<String, Vec<mlua::RegistryKey>>>>,
    pub pending_commands: Arc<Mutex<Vec<CompositorCommand>>>,
}

impl ConfigManager {
    pub fn new(config_dir: Option<&Path>) -> Result<Self, LuaError> {
        let ctxt = Lua::new();

        let config_dir = if config_dir.is_none() {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("pattern")
        } else {
            config_dir.unwrap().to_path_buf()
        };

        if !config_dir.exists() {
            let _ = std::fs::create_dir_all(&config_dir);
        }

        Ok(Self {
            ctxt,
            config_dir,
            bindings_store: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(CompositorConfig::default())),
            hooks: Arc::new(Mutex::new(HashMap::new())),
            pending_commands: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn run_hook(&self, hook_name: &str) -> Result<(), mlua::Error> {
        let hooks = self.hooks.lock().unwrap();
        if let Some(keys) = hooks.get(hook_name) {
            for key in keys {
                let func: mlua::Function = self.ctxt.registry_value(key)?;
                func.call::<()>(())?;
            }
        }
        Ok(())
    }

    pub fn load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let globals = self.ctxt.globals();
        let pattern = self.ctxt.create_table()?;

        let actions = self.ctxt.create_table()?;

        let quit_fn = self
            .ctxt
            .create_function(|_, ()| Ok(CompositorCommand::Quit))?;
        actions.set("quit", quit_fn)?;

        let exec_cmd_fn = self
            .ctxt
            .create_function(|_, cmd| Ok(CompositorCommand::Exec { full_sh_cmd: cmd }))?;

        actions.set("exec_cmd", exec_cmd_fn)?;

        // Expose pattern.actions.window.close(id)
        let window_table = self.ctxt.create_table()?;
        let close_fn = self
            .ctxt
            .create_function(|_, id: Option<u32>| Ok(CompositorCommand::CloseWindow { id }))?;
        window_table.set("close", close_fn)?;

        // Expose pattern.actions.window.fullscreen(opts)
        let fullscreen_fn = self.ctxt.create_function(|_, opts: Option<mlua::Table>| {
            let mut id = None;
            let mut toggle = true;
            let mut value = false;

            if let Some(t) = opts {
                id = t.get::<u32>("id").ok();
                if let Ok(val) = t.get::<bool>("value") {
                    value = val;
                    toggle = false;
                }
                if let Ok(tgl) = t.get::<bool>("toggle").or_else(|_| t.get::<bool>("toogle")) {
                    toggle = tgl;
                }
            }

            Ok(CompositorCommand::FullscreenWindow { id, toggle, value })
        })?;
        window_table.set("fullscreen", fullscreen_fn)?;

        // Expose pattern.actions.window.move(opts)
        let move_fn = self.ctxt.create_function(|_, opts: mlua::Table| {
            let workspace = opts.get::<usize>("workspace")?;
            let target_ws = if workspace > 0 { workspace - 1 } else { 0 };
            let id = opts.get::<u32>("id").ok();
            Ok(CompositorCommand::MoveWindowToWorkspace {
                id,
                workspace: target_ws,
            })
        })?;
        window_table.set("move", move_fn)?;

        // Expose pattern.actions.window.drag()
        let drag_fn = self
            .ctxt
            .create_function(|_, ()| Ok(CompositorCommand::DragWindow))?;
        window_table.set("drag", drag_fn)?;

        // Expose pattern.actions.window.resize()
        let resize_fn = self
            .ctxt
            .create_function(|_, ()| Ok(CompositorCommand::ResizeWindow))?;
        window_table.set("resize", resize_fn)?;

        actions.set("window", window_table)?;

        // Expose pattern.actions.workspace.focus(opts) and pattern.actions.focus(opts)
        let workspace_table = self.ctxt.create_table()?;
        let focus_fn = self.ctxt.create_function(|_, opts: mlua::Table| {
            let mut id = opts.get::<usize>("id").ok();
            if id.is_none()
                && let Ok(ws) = opts.get::<usize>("workspace")
            {
                id = Some(if ws > 0 { ws - 1 } else { 0 });
            }
            let next = opts.get::<bool>("next").unwrap_or(false);
            let previous = opts.get::<bool>("previous").unwrap_or(false);
            Ok(CompositorCommand::FocusWorkspace { id, next, previous })
        })?;
        actions.set("focus", focus_fn.clone())?;
        workspace_table.set("focus", focus_fn)?;
        actions.set("workspace", workspace_table)?;

        pattern.set("actions", actions)?;
        pattern.set("config_dir", self.config_dir.clone())?;

        let bindings = self.bindings_store.clone();

        // TODO: read opts
        let bind_fn = self.ctxt.create_function(
            move |lua, (keys, value, _opts): (String, mlua::Value, mlua::Value)| {
                let stored_action = match value {
                    mlua::Value::UserData(ud) => {
                        let action = ud.borrow::<CompositorCommand>()?.clone();
                        StoredAction::Builtin(action)
                    }
                    mlua::Value::Function(func) => {
                        // Pin the function in the registry so it isn't garbage collected
                        let registry_key = lua.create_registry_value(func)?;
                        StoredAction::LuaCallback(registry_key)
                    }
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "Expected Action or function".into(),
                        ));
                    }
                };

                if let Some(key_pattern) = parse_keybind(&keys) {
                    let mut store = bindings.lock().unwrap();
                    store.insert(key_pattern, Arc::new(stored_action));
                } else {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Invalid keybind: {}",
                        keys
                    )));
                }
                Ok(())
            },
        )?;

        pattern.set("bind", bind_fn)?;

        let hooks = self.hooks.clone();
        let on_fn =
            self.ctxt
                .create_function(move |lua, (hook_name, value): (String, mlua::Value)| {
                    if let mlua::Value::Function(func) = value {
                        let registry_key = lua.create_registry_value(func)?;
                        let mut hks = hooks.lock().unwrap();
                        hks.entry(hook_name).or_default().push(registry_key);
                        Ok(())
                    } else {
                        Err(mlua::Error::RuntimeError(
                            "Expected function for hook callback".into(),
                        ))
                    }
                })?;

        pattern.set("on", on_fn)?;

        let config = self.config.clone();
        let config_fn = self.ctxt.create_function(move |_, table: mlua::Table| {
            let mut cfg = config.lock().unwrap();

            macro_rules! update_field {
                ($table:expr, $cfg_field:expr, $key:expr, $type:ty) => {
                    if let Ok(val) = $table.get::<$type>($key) {
                        $cfg_field = val;
                    }
                };
            }

            macro_rules! update_table {
                ($parent_table:expr, $key:expr, $sub_table:ident => $body:block) => {
                    if let Ok($sub_table) = $parent_table.get::<mlua::Table>($key) {
                        $body
                    }
                };
            }

            update_table!(table, "input", input_table => {
                update_field!(input_table, cfg.input.kb_layout, "kb_layout", String);
                update_field!(input_table, cfg.input.kb_variant, "kb_variant", String);
                update_field!(input_table, cfg.input.kb_model, "kb_model", String);
                update_field!(input_table, cfg.input.kb_options, "kb_options", String);
                update_field!(input_table, cfg.input.kb_rules, "kb_rules", String);
                update_field!(input_table, cfg.input.repeat_rate, "repeat_rate", u32);
                update_field!(input_table, cfg.input.repeat_delay, "repeat_delay", u32);
                update_field!(input_table, cfg.input.sensitivity, "sensitivity", f64);

                update_table!(input_table, "touchpad", touchpad_table => {
                    update_field!(touchpad_table, cfg.input.natural_scroll, "natural_scroll", bool);
                    update_field!(touchpad_table, cfg.input.disable_while_typing, "disable_while_typing", bool);
                });
            });

            update_table!(table, "gestures", gestures_table => {
                update_field!(gestures_table, cfg.gestures.workspace_swipe_invert, "workspace_swipe_invert", bool);
                update_field!(gestures_table, cfg.gestures.workspace_swipe_threshold, "workspace_swipe_threshold", f64);
            });

            update_table!(table, "style", style_table => {
                update_field!(style_table, cfg.style.active_opacity, "active_opacity", f64);
                update_field!(style_table, cfg.style.inactive_opacity, "inactive_opacity", f64);
                update_field!(style_table, cfg.style.fullscreen_opacity, "fullscreen_opacity", f64);
                update_field!(style_table, cfg.style.dim_inactive, "dim_inactive", bool);
                update_field!(style_table, cfg.style.dim_strength, "dim_strength", f64);
                update_field!(style_table, cfg.style.rounding, "rounding", f32);

                update_table!(style_table, "blur", blur_table => {
                    update_field!(blur_table, cfg.style.blur.enabled, "enabled", bool);
                    update_field!(blur_table, cfg.style.blur.size, "size", u32);
                    update_field!(blur_table, cfg.style.blur.passes, "passes", u32);
                    update_field!(blur_table, cfg.style.blur.vibrancy, "vibrancy", f32);
                });

                update_table!(style_table, "shadow", shadow_table => {
                    update_field!(shadow_table, cfg.style.shadow.enabled, "enabled", bool);
                    update_field!(shadow_table, cfg.style.shadow.range, "range", u32);
                    update_field!(shadow_table, cfg.style.shadow.render_power, "render_power", u32);
                    update_field!(shadow_table, cfg.style.shadow.sharp, "sharp", bool);

                    update_table!(shadow_table, "color", color_table => {
                        let mut color = cfg.style.shadow.color;
                        for i in 0..4 {
                            if let Ok(c) = color_table.get::<f32>(i + 1) {
                                color[i] = c;
                            }
                        }
                        cfg.style.shadow.color = color;
                    });

                    update_table!(shadow_table, "offset", offset_table => {
                        let mut offset = cfg.style.shadow.offset;
                        if let Ok(x) = offset_table.get::<f64>(1) {
                            offset.0 = x;
                        }
                        if let Ok(y) = offset_table.get::<f64>(2) {
                            offset.1 = y;
                        }
                        cfg.style.shadow.offset = offset;
                    });
                });
            });

            Ok(())
        })?;

        pattern.set("config", config_fn)?;

        let config = self.config.clone();
        let gesture_fn = self.ctxt.create_function(move |_, table: mlua::Table| {
            let mut cfg = config.lock().unwrap();

            let fingers = table.get::<u32>("fingers")?;
            let direction = table.get::<String>("direction").unwrap_or_default();
            let action = table.get::<String>("action").unwrap_or_default();

            cfg.registered_gestures.push(GestureConfig {
                fingers,
                direction,
                action,
            });

            Ok(())
        })?;

        pattern.set("gesture", gesture_fn)?;

        let exec_cmd_fn = self.ctxt.create_function(|_, cmd: String| {
            std::process::Command::new("sh")
                .args(["-c", &cmd])
                .spawn()
                .map_err(|e| {
                    mlua::Error::RuntimeError(format!("Failed to spawn command: {}", e))
                })?;
            Ok(())
        })?;

        pattern.set("exec_cmd", exec_cmd_fn)?;

        let pending_commands = self.pending_commands.clone();
        let dispatch_fn = self.ctxt.create_function(move |_, value: mlua::Value| {
            match value {
                mlua::Value::UserData(ud) => {
                    let cmd = ud.borrow::<CompositorCommand>()?.clone();
                    match cmd {
                        CompositorCommand::Quit => {
                            std::process::exit(0);
                        }
                        CompositorCommand::Exec { full_sh_cmd } => {
                            let _ = std::process::Command::new("sh")
                                .args(["-c", &full_sh_cmd])
                                .spawn();
                        }
                        other => {
                            let mut queue = pending_commands.lock().unwrap();
                            queue.push(other);
                        }
                    }
                }
                mlua::Value::Function(func) => {
                    func.call::<()>(())?;
                }
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "Expected Action or function to dispatch".into(),
                    ));
                }
            }
            Ok(())
        })?;

        pattern.set("dispatch", dispatch_fn)?;

        globals.set("pattern", pattern)?;

        let init_path = self.config_dir.join("init.lua");
        let meta_path = self.config_dir.join("pattern.meta.lua");
        if !init_path.exists() {
            fs::write(&init_path, include_str!("default_init.lua"))?;
        }

        fs::write(&meta_path, include_str!("pattern.meta.lua"))?;

        let lua_code = fs::read_to_string(&init_path)?;
        self.ctxt.load(&lua_code).exec()?;

        Ok(())
    }
}
