use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::prelude::*;
pub mod keybinds;

#[derive(Clone, Debug)]
pub enum CompositorCommand {
    Quit,
    Spawn {
        cmd: String,
        args: Option<Vec<String>>,
    },
}

impl LuaUserData for CompositorCommand {}

pub enum StoredAction {
    Builtin(CompositorCommand),
    LuaCallback(mlua::RegistryKey),
}

type BindingsStore = Arc<Mutex<HashMap<String, StoredAction>>>;

pub struct ConfigManager {
    pub bindings_store: BindingsStore,
    pub config_dir: PathBuf,
    pub ctxt: Lua,
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
        })
    }

    pub fn load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let globals = self.ctxt.globals();
        let pattern = self.ctxt.create_table()?;

        let actions = self.ctxt.create_table()?;

        let quit_fn = self
            .ctxt
            .create_function(|_, ()| Ok(CompositorCommand::Quit))?;
        actions.set("quit", quit_fn)?;

        let spawn_fn =
            self.ctxt
                .create_function(|_, (cmd, args): (String, Option<Vec<String>>)| {
                    Ok(CompositorCommand::Spawn { cmd, args })
                })?;

        actions.set("spawn", spawn_fn)?;

        pattern.set("actions", actions)?;
        pattern.set("config_dir", self.config_dir.clone())?;

        let bindings = self.bindings_store.clone();

        let bind_fn =
            self.ctxt
                .create_function(move |lua, (keys, value): (String, mlua::Value)| {
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

                    let mut store = bindings.lock().unwrap();
                    store.insert(keys, stored_action);
                    Ok(())
                })?;

        pattern.set("bind", bind_fn)?;
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
