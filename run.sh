#!/usr/bin/fish
env VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation RUST_LOG_STYLE=never RUST_BACKTRACE=1 cargo run &>pattern.log
