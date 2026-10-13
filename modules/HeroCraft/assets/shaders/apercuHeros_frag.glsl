#version 330 core
// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0

// The creator's preview: the hero's sheet with its alpha cut, as on the hero mesh in the world, and a light that
// never lets a face go black — the figure turns, and a side seen edge-on is still a side.

uniform vec4 croppingBoundaries;
uniform sampler2D tex;

in vec3 v_normal;
in vec2 v_relPos;
in vec2 v_uv0;
in vec4 v_color0;

layout(location = 0) out vec4 outColor;

void main(){
    if (v_relPos.x < croppingBoundaries.x || v_relPos.x > croppingBoundaries.y || v_relPos.y < croppingBoundaries.z || v_relPos.y > croppingBoundaries.w) {
        discard;
    }
    vec4 color = texture(tex, v_uv0);
    if (color.a < 0.5) {
        discard;
    }
    float light = min(1.0, 0.62 + 0.38 * abs(v_normal.z) + 0.12 * abs(v_normal.y));
    outColor = vec4(color.rgb * light, 1.0) * v_color0;
}
