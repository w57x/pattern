#!/usr/bin/fish
cargo run --release &>pattern.log &
sleep 10 && WAYLAND_DISPLAY=wayland-0 kitty
