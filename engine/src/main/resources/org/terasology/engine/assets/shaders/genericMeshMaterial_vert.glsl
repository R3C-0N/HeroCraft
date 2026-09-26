#version 330 core
// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0

layout (location = 0) in vec3 in_vert;
layout (location = 1) in vec3 in_normal;
layout (location = 2) in vec2 in_uv0;
layout (location = 4) in vec4 in_color0;
// The skinning of a skeletal mesh: up to four bones per vertex, and how much each one counts. See OpenGLSkeletalMesh.
layout (location = 6) in uvec4 in_joints;
layout (location = 7) in vec4 in_weights;

uniform mat3 normalMatrix;
uniform mat4 modelViewMatrix;
uniform mat4 projectionMatrix;

// Mirrors OpenGLSkeletalMesh.MAX_BONES. 48 matrices are 768 of the 1024 uniform components GL 3.3 guarantees.
const int MAX_BONES = 48;
// Zero for anything that is not skinned here: a static mesh, or a skeletal mesh the CPU has already skinned.
uniform int boneCount;
uniform mat4 boneMatrices[MAX_BONES];

out vec3 v_normal;
out vec2 v_uv0;
out vec4 v_color0;

mat4 boneAt(uint joint) {
    return boneMatrices[min(int(joint), MAX_BONES - 1)];
}

void main() {
    vec4 position = vec4(in_vert, 1.0);
    vec3 normal = in_normal;
    if (boneCount > 0) {
        mat4 skin = in_weights.x * boneAt(in_joints.x) + in_weights.y * boneAt(in_joints.y)
                  + in_weights.z * boneAt(in_joints.z) + in_weights.w * boneAt(in_joints.w);
        // Back to w = 1, as the CPU path had it: glTF only asks for weights that sum to one, and a sum of 0.99 left in
        // w would blow that vertex up by a hundredth away from the creature's origin.
        position = vec4((skin * position).xyz, 1.0);
        // The linear part only: a normal is a direction, and the bones' translation must not reach it.
        normal = mat3(skin) * normal;
    }
	gl_Position = (projectionMatrix * modelViewMatrix) * position;

    v_normal = normalMatrix * normal;
    if (boneCount > 0) {
        // The scale of the creature is in the model-view here, and the normal matrix divides by it.
        v_normal = normalize(v_normal);
    }
    v_uv0 = in_uv0;
    v_color0 = in_color0;
}
