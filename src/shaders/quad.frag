#version 450

layout(location = 0) in vec2 fragTexCoord;
layout(location = 1) in vec2 fragQuadSize;
layout(location = 2) in float fragBorderRadius;

layout(location = 0) out vec4 outColor;

layout(binding = 0) uniform sampler2D texSampler;

// Rounded box SDF
float sdRoundedBox(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

void main() {
    // Convert UV to pixel coordinates relative to center
    vec2 p = (fragTexCoord - 0.5) * fragQuadSize;
    vec2 b = fragQuadSize * 0.5;
    
    float alpha = 1.0;
    if (fragBorderRadius > 0.0) {
        float d = sdRoundedBox(p, b, fragBorderRadius);
        // Antialiasing
        alpha = 1.0 - smoothstep(-1.0, 1.0, d);
    }
    
    vec4 texColor = texture(texSampler, fragTexCoord);
    outColor = texColor * alpha;
}
