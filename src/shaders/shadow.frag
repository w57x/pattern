#version 450

layout(push_constant) uniform PushConstants {
    vec2 pos;
    vec2 screen_size;
    vec2 quad_size;
    vec2 src_offset;
    vec2 src_size;
    float border_radius;
    float alpha;
    float shadow_spread;
    float shadow_power;
    vec4 color; // Shadow color
} pc;

layout(location = 0) in vec2 fragTexCoord;
layout(location = 1) in vec2 fragQuadSize;
layout(location = 2) in float fragBorderRadius;
layout(location = 3) in float fragAlpha;

layout(location = 0) out vec4 outColor;

// Box SDF
float sdRoundedBox(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

void main() {
    // Coordinate relative to the center of the quad
    vec2 quadUv = (fragTexCoord - pc.src_offset) / max(pc.src_size, vec2(0.001));
    vec2 p = (quadUv - 0.5) * fragQuadSize;

    float shadow_spread = pc.shadow_spread;
    vec2 b = (fragQuadSize * 0.5) - shadow_spread;

    float d = sdRoundedBox(p, b, fragBorderRadius);

    // We want a smooth gradient from d=0 (edge of window) to d=shadow_spread (edge of quad).
    // If d < 0, we are inside the window.

    float shadow_alpha = 1.0 - smoothstep(0.0, shadow_spread, d);

    // Cut out the shadow inside the window to prevent darkening the background 
    // behind translucent windows.
    if (d < 0.0) {
        shadow_alpha = 0.0;
    }

    // Apply power curve
    shadow_alpha = pow(shadow_alpha, pc.shadow_power);

    // Because Vulkan is configured with ONE / ONE_MINUS_SRC_ALPHA blending (pre-multiplied alpha),
    // we MUST pre-multiply the RGB by the final alpha here.

    outColor = pc.color;
    outColor.a *= shadow_alpha * fragAlpha;
    outColor.rgb *= outColor.a;
}
