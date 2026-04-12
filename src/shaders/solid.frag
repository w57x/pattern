#version 450

layout(push_constant) uniform PushConstants {
    vec2 pos;
    vec2 screen_size;
    vec2 quad_size;
    vec4 color;
} push;

layout(location = 0) out vec4 outColor;

void main() {
    outColor = push.color;
}