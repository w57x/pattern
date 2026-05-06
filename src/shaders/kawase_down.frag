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
    vec4 color;
} pc;

layout(location = 0) in vec2 fragTexCoord;
layout(location = 1) in vec2 fragQuadSize;
layout(location = 2) in float fragBorderRadius;
layout(location = 3) in float fragAlpha;

layout(location = 0) out vec4 outColor;

layout(binding = 0) uniform sampler2D texSampler;

void main() {
    vec2 uv = fragTexCoord;
    vec2 halfpixel = 0.5 / textureSize(texSampler, 0);

    vec4 sum = texture(texSampler, uv) * 4.0;
    sum += texture(texSampler, uv - halfpixel.xy);
    sum += texture(texSampler, uv + halfpixel.xy);
    sum += texture(texSampler, uv + vec2(halfpixel.x, -halfpixel.y));
    sum += texture(texSampler, uv - vec2(halfpixel.x, -halfpixel.y));

    outColor = sum / 8.0;
}