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

// Rounded box SDF
float sdRoundedBox(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

// Random noise for dithering (film grain)
float rand(vec2 co) {
    return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}

// Hyprland Vibrancy Logic constants
const float Pr = 0.299;
const float Pg = 0.587;
const float Pb = 0.114;
const float a_val = 0.93;
const float b_val = 0.11;
const float c_val = 0.66;

float doubleCircleSigmoid(float x, float a) {
    a = clamp(a, 0.0, 1.0);
    float y = 0.0;
    if (x <= a) {
        y = a - sqrt(max(a * a - x * x, 0.0));
    } else {
        y = a + sqrt(max(pow(1.0 - a, 2.0) - pow(x - 1.0, 2.0), 0.0));
    }
    return y;
}

vec3 rgb2hsl(vec3 col) {
    float red   = col.r;
    float green = col.g;
    float blue  = col.b;

    float minc  = min(col.r, min(col.g, col.b));
    float maxc  = max(col.r, max(col.g, col.b));
    float delta = maxc - minc;

    float lum = (minc + maxc) * 0.5;
    float sat = 0.0;
    float hue = 0.0;

    if (lum > 0.0 && lum < 1.0) {
        float mul = (lum < 0.5) ? (lum) : (1.0 - lum);
        sat       = delta / (mul * 2.0);
    }

    if (delta > 0.0) {
        vec3 maxcVec = vec3(maxc);
        vec3 masks   = vec3(equal(maxcVec, col)) * vec3(notEqual(maxcVec, vec3(green, blue, red)));
        vec3 adds    = vec3(0.0, 2.0, 4.0) + vec3(green - blue, blue - red, red - green) / delta;

        hue += dot(adds, masks);
        hue /= 6.0;

        if (hue < 0.0) hue += 1.0;
    }

    return vec3(hue, sat, lum);
}

vec3 hsl2rgb(vec3 col) {
    const float onethird = 1.0 / 3.0;
    const float twothird = 2.0 / 3.0;
    const float rcpsixth = 6.0;

    float hue = col.x;
    float sat = col.y;
    float lum = col.z;

    vec3 xt = vec3(0.0);

    if (hue < onethird) {
        xt.r = rcpsixth * (onethird - hue);
        xt.g = rcpsixth * hue;
        xt.b = 0.0;
    } else if (hue < twothird) {
        xt.r = 0.0;
        xt.g = rcpsixth * (twothird - hue);
        xt.b = rcpsixth * (hue - onethird);
    } else {
        xt = vec3(rcpsixth * (hue - twothird), 0.0, rcpsixth * (1.0 - hue));
    }

    xt = min(xt, 1.0);

    float sat2   = 2.0 * sat;
    float satinv = 1.0 - sat;
    float luminv = 1.0 - lum;
    float lum2m1 = (2.0 * lum) - 1.0;
    vec3  ct     = (sat2 * xt) + satinv;

    vec3 rgb;
    if (lum >= 0.5) rgb = (luminv * ct) + lum2m1;
    else            rgb = lum * ct;

    return rgb;
}

void main() {
    vec2 quadUv = (fragTexCoord - pc.src_offset) / max(pc.src_size, vec2(0.001));
    vec2 p = (quadUv - 0.5) * fragQuadSize;
    vec2 b = fragQuadSize * 0.5;

    float edgeAlpha = 1.0;
    if (fragBorderRadius > 0.0) {
        float d = sdRoundedBox(p, b, fragBorderRadius);
        // Use a sharper smoothing constant similar to Hyprland for slightly sharper corners
        edgeAlpha = 1.0 - smoothstep(-0.6, 0.6, d);
    }

    vec4 color = texture(texSampler, fragTexCoord);

    // Hardcode some nice vibrancy values for the glass effect.
    float vibrancy = 0.4;
    float vibrancy_darkness = 0.2;
    float passes = 4.0; // Assume an average of 4 passes for the boost calculation

    // Un-premultiply for color calculations if necessary, but we can assume mostly opaque background
    vec3 rgb_unpremult = color.a > 0.0 ? color.rgb / color.a : vec3(0.0);

    // Apply Vibrancy
    float vibrancy_darkness1 = 1.0 - vibrancy_darkness;
    vec3 hsl = rgb2hsl(rgb_unpremult);
    float perceivedBrightness = doubleCircleSigmoid(sqrt(rgb_unpremult.r * rgb_unpremult.r * Pr + rgb_unpremult.g * rgb_unpremult.g * Pg + rgb_unpremult.b * rgb_unpremult.b * Pb), 0.8 * vibrancy_darkness1);

    float b1 = b_val * vibrancy_darkness1;
    float boostBase = hsl[1] > 0.0 ? smoothstep(b1 - c_val * 0.5, b1 + c_val * 0.5, 1.0 - (pow(1.0 - hsl[1] * cos(a_val), 2.0) + pow(1.0 - perceivedBrightness * sin(a_val), 2.0))) : 0.0;

    float saturation = clamp(hsl[1] + (boostBase * vibrancy) / passes, 0.0, 1.0);
    vec3 newColor = hsl2rgb(vec3(hsl[0], saturation, hsl[2]));

    // Film Grain / Dithering
    // We add a tiny amount of noise to break up color banding.
    float noise = (rand(gl_FragCoord.xy) - 0.5) * 0.04;
    newColor = clamp(newColor + noise, 0.0, 1.0);

    // Re-premultiply and mix with the subtle tint (pc.color) exactly as before
    vec4 glassColor = vec4(newColor * color.a, color.a);
    vec4 finalColor = mix(glassColor, vec4(pc.color.rgb * pc.color.a, pc.color.a), pc.color.a);

    outColor = finalColor;
    outColor.a *= edgeAlpha * fragAlpha;
    outColor.rgb *= outColor.a;
}
