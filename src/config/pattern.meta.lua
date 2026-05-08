---@meta

--- @class Pattern
pattern = {}

---Allows you to add bindings.
---@param keys string The key chord (e.g., "SUPER + Q")
---@param cb fun() The action to execute when triggered
---@param opts? table Additional bind options
function pattern.bind(keys, cb, opts) end

---Dispatch an action
---@param action_cb fun() The action to execute
function pattern.dispatch(action_cb) end

---@class DefinedActions
---Predefined actions
pattern.actions = {}

---Quits the compositor.
---@return fun()
function pattern.actions.quit() end

---Spawns a new process.
---@param cmd string The command to run (e.g., "seekr")
---@param args? table Additional arguments
---@return fun()
function pattern.actions.spawn(cmd, args) end
