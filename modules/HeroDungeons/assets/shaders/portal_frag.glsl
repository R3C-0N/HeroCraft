#version 330 core
// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0

// The other side of a window, drawn beforehand from the eye moved through it, read back at this pixel's place on the
// screen; and round it a rim of shifting light, so that the opening reads as magic and not as a hole in the wall.
// The normal and light buffers are left as they are: written with no alpha, under blending.

uniform sampler2D portalView;
uniform vec2 invViewSize;
uniform float opacity;
// The rim's share of the face, across and up, and the seconds that drive its shimmer.
uniform vec2 border;
uniform float glowTime;

in vec2 v_uv;

layout(location = 0) out vec4 outColor;
layout(location = 1) out vec4 outNormal;
layout(location = 2) out vec4 outLight;

void main() {
    outNormal = vec4(0.0);
    outLight = vec4(0.0);
    // 0 on the outer edge of the rim, 1 where the opening starts, more inside it.
    vec2 depth = min(v_uv, 1.0 - v_uv) / border;
    float edge = min(depth.x, depth.y);

    // Light running round the frame: an angle round the centre, so the bands chase each other along all four sides.
    float angle = atan(v_uv.y - 0.5, v_uv.x - 0.5);
    float wave = 0.5 + 0.5 * sin(angle * 7.0 - glowTime * 2.2 + edge * 5.0);
    float flicker = 0.85 + 0.15 * sin(glowTime * 9.0 + angle * 3.0);
    // Past the tone mapping, values above one bloom into a glow: the frame shines a little even at night.
    vec3 glow = mix(vec3(0.45, 0.75, 2.4), vec3(1.9, 0.7, 2.6), wave) * flicker;

    if (edge < 1.0) {
        // Brightest against the opening, fading out to nothing at the rim's edge.
        float strength = smoothstep(0.0, 1.0, edge);
        outColor = vec4(glow, strength * strength * (0.55 + 0.45 * wave));
        return;
    }
    vec3 picture = texture(portalView, gl_FragCoord.xy * invViewSize).rgb;
    // The inner edge of the picture takes a little of the glow, so frame and opening are one thing.
    float halo = exp(-(edge - 1.0) * 3.0) * 0.45;
    outColor = vec4(mix(picture, glow, halo * wave), opacity);
}
