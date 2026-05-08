--- @meta './pattern.meta.lua'

-- Default Pattern Compositor Configuration

local p = pattern

-- Binding to actions
-- Actions are predefined configurable functions

p.bind("SUPER + Q", p.actions.quit())
p.bind("SUPER + T", p.actions.spawn("kitty"))
p.bind("SUPER + S", p.actions.spawn("seekr"))
