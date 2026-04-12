#version 450

layout(push_constant) uniform PushConstants {
    vec2 pos;
    vec2 screen_size;
    vec2 quad_size;
    vec2 src_offset;
    vec2 src_size;
    float border_radius;
    float _padding;
    vec4 color;
} push;

layout(location = 0) in vec2 fragTexCoord;
layout(location = 1) in vec2 fragQuadSize;
layout(location = 2) in float fragBorderRadius;

layout(location = 0) out vec4 outColor;

// Rounded box SDF
float sdRoundedBox(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

void main() {
    // Convert UV to pixel coordinates relative to center
    vec2 p = (fragTexCoord - 0.5) * fragQuadSize;
    vec2 b = fragQuadSize * 0.5;
    
    float d = sdRoundedBox(p, b, fragBorderRadius);
    
    // Antialiasing
    float alpha = 1.0 - smoothstep(-1.0, 1.0, d);
    
    outColor = push.color;
    outColor.a *= alpha;
}
