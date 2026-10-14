#version 330 core
// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0

// The face of a window, stretched from a unit square over its four corners, given from the eye: bottom left, bottom
// right, top right, top left. On the planet the corners are already bent, as the blocks round the window are.

layout (location = 0) in vec3 in_vert;

uniform vec3 corner0;
uniform vec3 corner1;
uniform vec3 corner2;
uniform vec3 corner3;

uniform mat4 modelViewMatrix;
uniform mat4 projectionMatrix;

void main() {
    vec3 bottom = mix(corner0, corner1, in_vert.x);
    vec3 top = mix(corner3, corner2, in_vert.x);
    gl_Position = projectionMatrix * modelViewMatrix * vec4(mix(bottom, top, in_vert.y), 1.0);
}
