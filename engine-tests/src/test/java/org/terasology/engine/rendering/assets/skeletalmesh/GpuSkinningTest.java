// Copyright 2026 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.engine.rendering.assets.skeletalmesh;

import gnu.trove.list.array.TIntArrayList;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector2f;
import org.joml.Vector3f;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.terasology.joml.geom.AABBf;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether a creature skinned by the shader stands where the CPU used to put it.
 * <p>
 * The skinning moved from SkeletalMeshData.getVertexPositions, run for every vertex of every creature every frame, to
 * genericMeshMaterial_vert.glsl. {@link #shaderSkin} mirrors the shader line for line, from the matrices that
 * {@link SkeletalMeshData#skinMatrices} builds down the hierarchy; the reference is the CPU path, fed as it was, with
 * each bone's transform taken by walking up its chain as LocationComponent.getRelativeTransform did, and transposed.
 * <p>
 * The skeleton is built so that every convention shows: the bones are listed child first, every bone has a rest
 * rotation and a rest offset, and a vertex follows up to three bones. A test on bones at rest would agree whatever
 * the order of the products, since every skinning matrix is then the identity.
 */
class GpuSkinningTest {

    private Bone root;
    private Bone arm;
    private Bone hand;
    private SkeletalMeshData data;
    private Matrix4f[] restLocal;
    private Matrix4f[] poseLocal;

    @BeforeEach
    void buildSkeleton() {
        restLocal = new Matrix4f[] {
                new Matrix4f().translationRotateScale(new Vector3f(0, 2, 0.5f), new Quaternionf().rotateX(0.8f), 1),
                new Matrix4f().translationRotateScale(new Vector3f(0.3f, 1, 0), new Quaternionf().rotateY(0.5f), 1),
                new Matrix4f().translationRotateScale(new Vector3f(0, 1.5f, 0), new Quaternionf().rotateZ(-0.6f), 1)};
        poseLocal = new Matrix4f[] {
                new Matrix4f().translationRotateScale(new Vector3f(0.2f, 2, 0.4f), new Quaternionf().rotateX(1.3f).rotateY(0.2f), 1),
                new Matrix4f().translationRotateScale(new Vector3f(1.3f, 1.2f, -0.4f), new Quaternionf().rotateY(1.4f), 1),
                new Matrix4f().translationRotateScale(new Vector3f(0, 1.1f, 0.3f), new Quaternionf().rotateZ(0.9f), 1)};
        arm = new Bone(0, "arm", restLocal[0]);
        root = new Bone(1, "root", restLocal[1]);
        hand = new Bone(2, "hand", restLocal[2]);
        root.addChild(arm);
        arm.addChild(hand);
        for (Bone bone : new Bone[] {root, arm, hand}) {
            bone.setInverseBindMatrix(chain(bone, restLocal).invert());
        }

        List<Vector3f> vertices = Arrays.asList(
                new Vector3f(0.1f, 4.4f, 0.9f), new Vector3f(0.8f, 4.1f, 1.2f), new Vector3f(0.2f, 4.9f, 1.6f),
                new Vector3f(0.5f, 1.2f, 0.1f), new Vector3f(1.1f, 2.6f, 0.4f), new Vector3f(0.4f, 3.1f, -0.2f));
        List<BoneWeight> weights = Arrays.asList(
                new BoneWeight(new float[] {1}, new int[] {2}),
                new BoneWeight(new float[] {0.6f, 0.4f}, new int[] {2, 0}),
                new BoneWeight(new float[] {0.5f, 0.3f, 0.2f}, new int[] {2, 0, 1}),
                new BoneWeight(new float[] {1}, new int[] {1}),
                new BoneWeight(new float[] {0.7f, 0.3f}, new int[] {0, 1}),
                new BoneWeight(new float[] {0.25f, 0.25f, 0.5f}, new int[] {1, 2, 0}));
        List<Vector3f> normals = new ArrayList<>();
        List<Vector2f> uvs = new ArrayList<>();
        for (int i = 0; i < vertices.size(); i++) {
            normals.add(new Vector3f(0, 1, 0));
            uvs.add(new Vector2f());
        }
        data = new SkeletalMeshData(Arrays.asList(hand, arm, root), vertices, normals, weights, uvs,
                new TIntArrayList(new int[] {0, 1, 2, 3, 4, 5}), new AABBf(0, 0, 0, 1, 1, 1));
    }

    /** Where a bone stands in the space of the entity, walked up its chain as getRelativeTransform walks it. */
    private static Matrix4f chain(Bone bone, Matrix4f[] locals) {
        Matrix4f out = bone.getParent() == null ? new Matrix4f() : chain(bone.getParent(), locals);
        return out.mul(locals[bone.getIndex()]);
    }

    private Matrix4f[] skinMatrices(Matrix4f[] locals) {
        Matrix4f[] model = new Matrix4f[3];
        Matrix4f[] skin = new Matrix4f[3];
        for (int i = 0; i < 3; i++) {
            model[i] = new Matrix4f();
            skin[i] = new Matrix4f();
        }
        data.skinMatrices((bone, dest) -> {
            dest.set(locals[bone.getIndex()]);
            return true;
        }, model, skin);
        return skin;
    }

    /** The CPU path, fed as SkeletonRenderer fed it: each bone's chain times its inverse bind, transposed. */
    private List<Matrix4f> cpuTransforms(Matrix4f[] locals) {
        Matrix4f[] transforms = new Matrix4f[3];
        for (Bone bone : data.getBones()) {
            transforms[bone.getIndex()] = chain(bone, locals).mul(bone.getInverseBindMatrix()).transpose();
        }
        return Arrays.asList(transforms);
    }

    /**
     * genericMeshMaterial_vert.glsl, for one vertex: the weighted sum of its bones, every component of each matrix
     * scaled as a float times a mat4 scales it, and the w of the result put back to one.
     */
    private Matrix4f shaderSkin(int vertex, Matrix4f[] skin) {
        BoneWeight weight = data.getWeights().get(vertex);
        Matrix4f sum = new Matrix4f().zero();
        for (int w = 0; w < SkeletalMeshData.MAX_WEIGHTS; w++) {
            if (w < weight.jointCount()) {
                sum.fma4x3(skin[weight.getJoint(w)], weight.getBias(w));
            }
        }
        return sum.m33(1);
    }

    @Test
    void theBonesAreBuiltParentFirst() {
        List<Bone> order = data.getBonesParentFirst();
        assertEquals(Arrays.asList(root, arm, hand), order);
    }

    @Test
    void atRestTheShaderLeavesTheMeshAsLoaded() {
        Matrix4f[] skin = skinMatrices(restLocal);
        for (int i = 0; i < data.getVertexCount(); i++) {
            Vector3f p = shaderSkin(i, skin).transformPosition(new Vector3f(data.getRestPositions().get(i)));
            assertEquals(0, p.distance(data.getRestPositions().get(i)), 1e-5, "vertex " + i + " moved at rest");
        }
    }

    @Test
    void theShaderPutsEveryVertexWhereTheCpuDid() {
        Matrix4f[] skin = skinMatrices(poseLocal);
        List<Vector3f> cpu = data.getVertexPositions(cpuTransforms(poseLocal));
        double worst = 0;
        double moved = 0;
        for (int i = 0; i < data.getVertexCount(); i++) {
            Vector3f gpu = shaderSkin(i, skin).transformPosition(new Vector3f(data.getRestPositions().get(i)));
            worst = Math.max(worst, gpu.distance(cpu.get(i)));
            moved = Math.max(moved, gpu.distance(data.getRestPositions().get(i)));
        }
        System.out.printf(Locale.ROOT, "posed: %.2e blocks from the CPU, up to %.2f blocks from rest%n", worst, moved);
        assertTrue(moved > 1, "the pose barely moves the mesh, so agreeing proves nothing");
        assertEquals(0, worst, 1e-5, "the shader and the CPU disagree on a posed vertex");
    }

    @Test
    void aNormalTurnsWithItsBonesAndIsNotCarriedByThem() {
        Matrix4f[] skin = skinMatrices(poseLocal);
        List<Vector3f> cpu = data.getVertexNormals(cpuTransforms(poseLocal));
        double worst = 0;
        double shifted = 0;
        for (int i = 0; i < data.getVertexCount(); i++) {
            Vector3f rest = data.getRestNormals().get(i);
            Matrix4f skinOf = shaderSkin(i, skin);
            Vector3f gpu = skinOf.get3x3(new Matrix3f()).transform(new Vector3f(rest)).normalize();
            worst = Math.max(worst, gpu.distance(cpu.get(i)));
            // What the CPU path did before: the normal transformed as a point, the bones' offset added to it.
            Vector3f asPoint = new Vector3f(rest).mulTransposePosition(new Matrix4f(skinOf).transpose());
            shifted = Math.max(shifted, asPoint.distance(gpu));
        }
        System.out.printf(Locale.ROOT, "normals: %.2e from the CPU; transformed as points they were %.2f off%n",
                worst, shifted);
        assertTrue(shifted > 0.5, "the bones' offset is too small to tell a direction from a point");
        assertEquals(0, worst, 1e-5, "the shader and the CPU disagree on a normal");
    }
}
