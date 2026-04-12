#version 450

layout(push_constant) uniform PushConstants {
    vec2 pos;         // Quad X, Y
    vec2 screen_size; // Screen Width, Height
    vec2 quad_size;   // Size of the quad in pixels
} push;


// A normalized 1x1 square
vec2 positions[6] = vec2[](
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
);

// UV Texture Coordinates (0.0 to 1.0)
vec2 uvs[6] = vec2[](
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
);

layout(location = 0) out vec2 fragTexCoord;

void main() {
    vec2 p = (positions[gl_VertexIndex] * push.quad_size) + push.pos;

    vec2 ndc = (p / push.screen_size) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 0.0, 1.0);

    fragTexCoord = uvs[gl_VertexIndex];
}
