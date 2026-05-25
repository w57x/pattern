--- @meta './pattern.meta.lua'

-- Default Pattern Compositor Configuration

local p = pattern

-- add binding to a hook
p.on("@start", function()
	p.exec_cmd("qs")
	p.exec_cmd("fcitx5 -d")
	p.exec_cmd("seekr --silent")
end)

-- Add or update something to the configuartion fragments
p.config({
	input = {
		kb_layout = "be",
		kb_variant = "oss",
		kb_model = "",
		kb_options = "",
		kb_rules = "",
		repeat_rate = 30,
		repeat_delay = 500,

		sensitivity = 0.2, -- -1.0 - 1.0, 0 means no modification.

		touchpad = {
			natural_scroll = true,
			disable_while_typing = false,
		},
	},

	gestures = {
		workspace_swipe_invert = true,
		workspace_swipe_threshold = 300.0,
	},
})

-- we register a gesture
p.gesture({
	fingers = 3,
	direction = "horizontal",
	action = "workspace",
})

-- Binding to actions
-- Actions are predefined configurable functions

p.bind("SUPER + Q", p.actions.quit())
p.bind("SUPER + T", p.actions.exec_cmd("kitty"))
p.bind("SUPER + S", p.actions.exec_cmd("seekr"))

-- Workspace navigation and window movement binds (SUPER + 1..10)
for i = 1, 10 do
    p.bind("SUPER + code:" .. (i + 9), p.actions.workspace.focus({ workspace = i }))
    p.bind("SUPER + SHIFT + code:" .. (i + 9), p.actions.window.move({ workspace = i }))
end

-- Mouse bindings for window dragging and resizing (BTN_LEFT=272, BTN_MIDDLE=274)
p.bind("SUPER + mouse:272", p.actions.window.drag(), { mouse = true })
p.bind("SUPER + mouse:274", p.actions.window.resize(), { mouse = true })
