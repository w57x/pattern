#version 450

layout(push_constant) uniform PushConstants {
    vec2 pos;         // Quad X, Y
    vec2 screen_size; // Screen Width, Height
    vec2 quad_size;   // Size of the quad in pixels
    vec2 src_offset;  // UV offset
    vec2 src_size;    // UV size
    float border_radius;
    float _padding;
    vec4 color;       // Color for solid quads
} push;

layout(location = 0) out vec2 fragTexCoord;
layout(location = 1) out vec2 fragQuadSize;
layout(location = 2) out float fragBorderRadius;

// A normalized 1x1 square
vec2 positions[6] = vec2[](
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0)
);

vec2 uvs[6] = vec2[](
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0)
);

void main() {
    vec2 p = (positions[gl_VertexIndex] * push.quad_size) + push.pos;

    vec2 ndc = (p / push.screen_size) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 0.0, 1.0);

    fragTexCoord = push.src_offset + (uvs[gl_VertexIndex] * push.src_size);
    fragQuadSize = push.quad_size;
    fragBorderRadius = push.border_radius;
}
