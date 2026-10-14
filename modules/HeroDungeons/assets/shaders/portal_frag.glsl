#version 330 core
// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0

// The other side of a window, drawn beforehand from the eye moved through it, read back at this pixel's place on the
// screen. The normal and light buffers are left as they are: written with no alpha, under blending.

uniform sampler2D portalView;
uniform vec2 invViewSize;
uniform float opacity;

layout(location = 0) out vec4 outColor;
layout(location = 1) out vec4 outNormal;
layout(location = 2) out vec4 outLight;

void main() {
    outColor = vec4(texture(portalView, gl_FragCoord.xy * invViewSize).rgb, opacity);
    outNormal = vec4(0.0);
    outLight = vec4(0.0);
}
