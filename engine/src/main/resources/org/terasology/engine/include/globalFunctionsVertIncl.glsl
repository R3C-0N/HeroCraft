// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0

float timeToTick(float time, float speed) {
    return time * 4000.0 * speed;
}

float smoothCurve(float x) {
  return x * x * (3.0 - 2.0 * x);
}

float triangleWave(float x) {
  return abs(fract(x + 0.5) * 2.0 - 1.0);
}

float smoothTriangleWave(float x) {
  return smoothCurve(triangleWave(x)) * 2.0 - 1.0;
}


// -- Curved worlds ------------------------------------------------------------------------------
//
// The projection table holds, per texel, the gnomonic pair (a, b) of the tangent plane and the
// tangential scale factor. The bilinear read below is written out by hand and must stay identical
// to the one on the CPU: that identity is the only reason a position computed there and a vertex
// placed here end up in the same spot.

#if __VERSION__ >= 130

const int SPHERE_FACES = 6;
const vec3 SPHERE_RIGHT[SPHERE_FACES] = vec3[SPHERE_FACES](
    vec3( 0.0,  1.0,  0.0), vec3(-1.0,  0.0,  0.0), vec3( 0.0, -1.0,  0.0),
    vec3( 1.0,  0.0,  0.0), vec3(-1.0,  0.0,  0.0), vec3(-1.0,  0.0,  0.0));
const vec3 SPHERE_UP[SPHERE_FACES] = vec3[SPHERE_FACES](
    vec3( 0.0,  0.0,  1.0), vec3( 0.0,  0.0,  1.0), vec3( 0.0,  0.0,  1.0),
    vec3( 0.0,  0.0,  1.0), vec3( 0.0, -1.0,  0.0), vec3( 0.0,  1.0,  0.0));
const vec3 SPHERE_NORMAL[SPHERE_FACES] = vec3[SPHERE_FACES](
    vec3( 1.0,  0.0,  0.0), vec3( 0.0,  1.0,  0.0), vec3(-1.0,  0.0,  0.0),
    vec3( 0.0, -1.0,  0.0), vec3( 0.0,  0.0,  1.0), vec3( 0.0,  0.0, -1.0));
// Slot of each face in the four by four net, as (column, row). Ten slots carry nothing.
const vec2 SPHERE_PATTERN[SPHERE_FACES] = vec2[SPHERE_FACES](
    vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(2.0, 1.0), vec2(3.0, 1.0), vec2(1.0, 2.0), vec2(1.0, 0.0));

// Where a vertex that must not be seen is sent: far below the focus, past any far plane.
const float SPHERE_EXILE = 1.0e7;

// Which face a world column belongs to, and where inside it. Returns -1 for a dead slot.
int sphereFaceOf(vec2 column, out vec2 local) {
    vec2 slot = floor(column / sphereFaceEdge);
    local = column - slot * sphereFaceEdge;
    for (int f = 0; f < SPHERE_FACES; f++) {
        if (slot == SPHERE_PATTERN[f]) {
            return f;
        }
    }
    return -1;
}

vec3 sphereTableAt(vec2 st) {
    float n = float(sphereTableSize - 1);
    float x = clamp((st.x + 1.0) * 0.5 * n, 0.0, n - 0.0001);
    float y = clamp((st.y + 1.0) * 0.5 * n, 0.0, n - 0.0001);
    ivec2 i = ivec2(int(x), int(y));
    vec2 f = vec2(x - float(i.x), y - float(i.y));
    vec3 a = texelFetch(sphereTable, i, 0).rgb;
    vec3 b = texelFetch(sphereTable, i + ivec2(1, 0), 0).rgb;
    vec3 c = texelFetch(sphereTable, i + ivec2(0, 1), 0).rgb;
    vec3 d = texelFetch(sphereTable, i + ivec2(1, 1), 0).rgb;
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// The unit direction of a world column, in planet space, plus its scale factor in w.
//
// A block is centred on its coordinates, so the outer vertices of the blocks on the edge of a face lie half a block
// past it. Over a glued edge that half block falls on the face next door, which places it where it belongs; over a
// cut seam it falls in a dead slot, and the vertex was thrown to a fixed direction on the far side of the planet —
// triangles stretched into a sheet standing down the seam, and a gap between the two faces through which the sky
// showed. Such a point is brought back onto the edge of the face it is half a block from. Mirrored on the CPU by
// CubeSphereProjection, in both its paths.
const vec2 SPHERE_NUDGES[8] = vec2[8](
    vec2(0.5, 0.0), vec2(-0.5, 0.0), vec2(0.0, 0.5), vec2(0.0, -0.5),
    vec2(0.5, 0.5), vec2(-0.5, 0.5), vec2(0.5, -0.5), vec2(-0.5, -0.5));

// The face of a column once brought out of a dead slot, or -1 when no face is within half a block: a pocket.
int sphereOwnFace(vec2 column, out vec2 local) {
    int face = sphereFaceOf(column, local);
    for (int i = 0; i < 8 && face < 0; i++) {
        face = sphereFaceOf(column + SPHERE_NUDGES[i], local);
    }
    return face;
}

vec4 sphereDirection(vec2 column) {
    vec2 local;
    int face = sphereOwnFace(column, local);
    if (face < 0) {
        return vec4(0.0, 0.0, 1.0, 1.0);
    }
    vec2 st = 2.0 * local / sphereFaceEdge - 1.0;
    vec3 ab = sphereTableAt(st);
    vec3 d = normalize(SPHERE_NORMAL[face] + SPHERE_RIGHT[face] * ab.x + SPHERE_UP[face] * ab.y);
    return vec4(d, ab.z);
}

// Declared here because the curved position below needs it and it reads better after.
mat3 sphereFrameAbs(vec3 worldPos);

// Where a world position ends up once the world is bent, relative to the focus.
//
// The height is multiplied by the scale factor, not added raw. A conformal map stretches both
// tangential directions by that factor while leaving the radial one alone, and a block would stop
// being cubic; scaling the height makes the deformation a local similarity instead.
//
// The focus is the same for the whole frame, so its bent point and its frame come from the CPU
// (sphereFocusPoint, sphereFocusFrame), computed there in single precision and in this order: a
// vertex used to recompute them itself, eleven of its seventeen table reads.
vec3 sphereCurvedPos(vec3 worldPos) {
    vec2 local;
    bool hereDead = sphereOwnFace(worldPos.xz, local) < 0;
    bool focusDead = sphereOwnFace(sphereFocus.xz, local) < 0;
    // A pocket: the dead slots of the net hold the instances of the past, and they are flat. Seen
    // from inside one, they are drawn as they are stored; anything on the other side of the line,
    // planet seen from a pocket or pocket seen from the planet, is sent out of every frustum. The
    // corner of a block on a cut seam lies half a block into a dead slot and is not a pocket.
    if (hereDead || focusDead) {
        return hereDead && focusDead ? worldPos - sphereFocus : vec3(0.0, -SPHERE_EXILE, 0.0);
    }
    vec4 here = sphereDirection(worldPos.xz);
    float r = sphereRadius + here.w * (worldPos.y - sphereReferenceHeight);
    // Back into world axes. The difference of two planet-space points is still planet space, and
    // the caller multiplies it by the linear part of a world-space view matrix: without this the
    // whole terrain is rotated, and at the middle of a face mirrored, out from under everything
    // that is not drawn through this function.
    return sphereFocusFrame * (here.xyz * r - sphereFocusPoint);
}

// The local frame of a world column, in planet space: world X, Y, Z map onto east, up, north.
//
// Planet space is not world space, and the two differ by a whole rotation — at the middle of face
// one by a reflection, determinant minus one. Nothing outside this file knows about planet space,
// so nothing outside may see a vector expressed in it. Both users below bring it back.
mat3 sphereFrameAbs(vec3 worldPos) {
    vec2 local;
    if (sphereOwnFace(worldPos.xz, local) < 0) {
        // A pocket is flat, and its frame is the world's. Left to the differences below, a dead slot
        // reads the same direction on every side and normalises a zero vector.
        return mat3(1.0);
    }
    float step = sphereFaceEdge / 256.0;
    vec3 up = sphereDirection(worldPos.xz).xyz;
    vec3 alongX = sphereDirection(worldPos.xz + vec2(step, 0.0)).xyz
                - sphereDirection(worldPos.xz - vec2(step, 0.0)).xyz;
    vec3 alongZ = sphereDirection(worldPos.xz + vec2(0.0, step)).xyz
                - sphereDirection(worldPos.xz - vec2(0.0, step)).xyz;
    vec3 east = normalize(alongX - up * dot(alongX, up));
    vec3 north = normalize(alongZ - up * dot(alongZ, up));
    return mat3(east, up, north);
}

// The frame that turns a flat normal into the curved one, expressed in world axes.
//
// Relative to the focus, which is what makes it the identity under the camera and a turn of d / R
// away from it — eleven degrees at a thousand blocks. The absolute frame would instead hand the
// renderer a planet-space normal while the sun stays in world space, and no amount of daylight
// would land on the right face.
mat3 sphereFrame(vec3 worldPos) {
    return sphereFocusFrame * sphereFrameAbs(worldPos);
}

// The whole point of the exercise, in one call.
//
// Every node that draws world geometry builds its model-view as V * T(origin - reference), a pure
// translation, so mat3(mv) is the linear part of the view and the translation can be recovered
// exactly. That is what lets the deformation happen here without touching a single matrix, and it
// keeps the head bob, the reflected camera and the orthographic light camera all working.
vec4 sphereViewPos(vec3 localVert, vec3 originWorld, mat4 mv) {
    if (sphereEnabled == 0) {
        return mv * vec4(localVert, 1.0);
    }
    mat3 linear = mat3(mv);
    vec3 translation = mv[3].xyz - linear * (originWorld - sphereModelOrigin);
    return vec4(linear * sphereCurvedPos(localVert + originWorld) + translation, 1.0);
}

#else

// texelFetch needs GLSL 130. Every shader in the tree asks for 330, but a shader with no #version
// at all falls back to 120, and one that cannot read the table must still compile.
vec4 sphereViewPos(vec3 localVert, vec3 originWorld, mat4 mv) {
    return mv * vec4(localVert, 1.0);
}

mat3 sphereFrame(vec3 worldPos) {
    return mat3(1.0);
}

#endif
