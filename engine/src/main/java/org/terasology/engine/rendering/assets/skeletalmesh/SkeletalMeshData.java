// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.engine.rendering.assets.skeletalmesh;

import com.google.common.collect.ImmutableList;
import com.google.common.collect.Lists;
import com.google.common.collect.Maps;
import gnu.trove.list.TIntList;
import gnu.trove.list.array.TIntArrayList;
import org.joml.Matrix4f;
import org.joml.Vector2f;
import org.joml.Vector3f;
import org.terasology.gestalt.assets.AssetData;
import org.terasology.joml.geom.AABBf;

import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;
import java.util.stream.IntStream;


public class SkeletalMeshData implements AssetData {

    /** The most bones a vertex may follow: what glTF carries in JOINTS_0 and WEIGHTS_0, and what the shader reads. */
    public static final int MAX_WEIGHTS = 4;

    private Bone rootBone;
    private Map<String, Bone> boneLookup = Maps.newHashMap();
    private List<Bone> bones = Lists.newArrayList();
    private List<Vector2f> uvs;
    private List<Vector3f> vertices;
    private List<Vector3f> normals;
    private List<BoneWeight> weights = Lists.newArrayList();
    private TIntList indices = new TIntArrayList();
    private AABBf staticAABB;
    private List<Bone> parentFirst;

    public SkeletalMeshData(List<Bone> bones, List<Vector3f> vertices, List<Vector3f> normals,
                            List<BoneWeight> weights, List<Vector2f> uvs, TIntList indices, AABBf staticAABB) {
        for (Bone bone : bones) {
            boneLookup.put(bone.getName(), bone);
            if (bone.getParent() == null) {
                rootBone = bone;
            }
        }
        this.bones.addAll(bones);
        this.weights.addAll(weights);
        this.uvs = ImmutableList.copyOf(uvs);
        this.vertices = ImmutableList.copyOf(vertices);
        this.normals = ImmutableList.copyOf(normals);
        this.indices.addAll(indices);
        this.staticAABB = staticAABB;
        this.parentFirst = parentFirst(bones);

        calculateNormals();
    }

    /** Every bone after its parent, so that a pose can be built in one pass down the hierarchy. */
    private static List<Bone> parentFirst(List<Bone> bones) {
        List<Bone> order = Lists.newArrayListWithCapacity(bones.size());
        List<Bone> pending = Lists.newArrayList(bones);
        while (!pending.isEmpty()) {
            int before = pending.size();
            pending.removeIf(bone -> {
                if (bone.getParent() == null || order.contains(bone.getParent())) {
                    order.add(bone);
                    return true;
                }
                return false;
            });
            if (pending.size() == before) {
                // A parent that is not a bone of this mesh: nothing below it can ever be placed after it.
                order.addAll(pending);
                break;
            }
        }
        return ImmutableList.copyOf(order);
    }

    /**
     * @return Information on all bones composing the mesh
     */
    public Collection<Bone> getBones() {
        return bones;
    }

    /**
     * @return All bones, each one after its parent
     */
    public List<Bone> getBonesParentFirst() {
        return parentFirst;
    }

    /**
     * Where a bone stands relative to its parent, or to the entity for the root.
     */
    @FunctionalInterface
    public interface BoneLocal {
        /**
         * @return false when the bone has nothing to say, in which case the vertices that follow it stay where the
         *         mesh has them at rest
         */
        boolean localTransform(Bone bone, Matrix4f dest);
    }

    /**
     * The skinning matrix of each bone: from the mesh at rest to where the bone stands now, in the space of the entity.
     * One pass down the hierarchy, each bone from its parent.
     *
     * @param localOf where each bone stands relative to its parent
     * @param model   scratch, one matrix per bone: where each bone stands in the space of the entity
     * @param skin    filled with one matrix per bone, indexed by {@link Bone#getIndex()}, as the shader takes them
     */
    public void skinMatrices(BoneLocal localOf, Matrix4f[] model, Matrix4f[] skin) {
        Matrix4f local = new Matrix4f();
        for (Bone bone : parentFirst) {
            int index = bone.getIndex();
            Matrix4f here = model[index];
            if (bone.getParent() != null) {
                here.set(model[bone.getParent().getIndex()]);
            } else {
                here.identity();
            }
            if (localOf.localTransform(bone, local.identity())) {
                here.mul(local);
                here.mul(bone.getInverseBindMatrix(), skin[index]);
            } else {
                skin[index].identity();
            }
        }
    }

    /**
     * @return The vertex positions as loaded, in the space of the mesh: where the bones put them at rest
     */
    public List<Vector3f> getRestPositions() {
        return vertices;
    }

    /**
     * @return The vertex normals at rest, in the space of the mesh
     */
    public List<Vector3f> getRestNormals() {
        return normals;
    }

    /**
     * @return The bones each vertex follows, and how much
     */
    public List<BoneWeight> getWeights() {
        return weights;
    }

    /**
     * @return Information on the root bone
     */
    public Bone getRootBone() {
        return rootBone;
    }

    /**
     * @return Provides the vertex positions for the default pose
     */
    public List<Vector3f> getBindPoseVertexPositions() {
        Matrix4f[] transforms = new Matrix4f[bones.size()];
        for (Bone bone : bones) {
            transforms[bone.getIndex()] = bone.getObjectTransform();
        }
        return getVertexPositions(Arrays.asList(transforms));
    }

    /**
     * @return Provides the vertex normals for the default pose
     */
    public List<Vector3f> getBindPoseVertexNormals() {
        Matrix4f[] transforms = new Matrix4f[bones.size()];
        for (Bone bone : bones) {
            transforms[bone.getIndex()] = bone.getObjectTransform();
        }
        return getVertexNormals(Arrays.asList(transforms));
    }

    /**
     * Provides the positions of all vertices of the mesh, transformed based on the transformation matrices of all
     * bones
     *
     * @param boneTransforms A transformation matrix for each bone in the skeletal mesh
     * @return The positions of each vertex
     */
    public List<Vector3f> getVertexPositions(List<Matrix4f> boneTransforms) {
        List<Vector3f> results = Lists.newArrayListWithCapacity(getVertexCount());
        for (int i = 0; i < vertices.size(); i++) {
            Vector3f pos = new Vector3f(vertices.get(i));
            Matrix4f skinMat = new Matrix4f().m00(0).m11(0).m22(0).m33(0);
            BoneWeight weight = weights.get(i);
            for (int w = 0; w < weight.jointCount(); w++) {
                Matrix4f jointMat = new Matrix4f(boneTransforms.get(weight.getJoint(w)));
                jointMat.scale(weight.getBias(w));
                skinMat.add(jointMat);
            }
            pos.mulTransposePosition(skinMat);
            results.add(pos);
        }
        return results;
    }

    /**
     * Provides the normals of all vertices of the mesh, transformed based on the transformation matrices of all bones
     *
     * @param boneTransforms A transformation matrix for each bone in the skeletal mesh
     * @return The normals of each vertex
     */
    public List<Vector3f> getVertexNormals(List<Matrix4f> boneTransforms) {
        List<Vector3f> results = Lists.newArrayListWithCapacity(getVertexCount());
        for (int i = 0; i < normals.size(); i++) {
            Vector3f norm = new Vector3f(normals.get(i));
            Matrix4f skinMat = new Matrix4f().m00(0).m11(0).m22(0).m33(0);
            BoneWeight weight = weights.get(i);
            for (int w = 0; w < weight.jointCount(); w++) {
                Matrix4f jointMat = new Matrix4f(boneTransforms.get(weight.getJoint(w)));
                jointMat.scale(weight.getBias(w));
                skinMat.add(jointMat);
            }
            // A direction, not a point: the translation of the bones has no business here. Transformed as a position,
            // every normal was shifted by the offset of its bone and the lighting of a creature depended on where its
            // limbs were.
            norm.mulTransposeDirection(skinMat).normalize();
            results.add(norm);
        }
        return results;
    }

    /**
     * @return The number of vertices composing the mesh
     */
    public int getVertexCount() {
        return vertices.size();
    }

    /**
     * @param name The name of the bone
     * @return Provides information for the named bone
     */
    public Bone getBone(String name) {
        return boneLookup.get(name);
    }

    /**
     * @return The indices instructing how to render the vertices as triangles
     */
    public TIntList getIndices() {
        return indices;
    }

    /**
     * @return The texture coordinate of each vertex
     */
    public List<Vector2f> getUVs() {
        return uvs;
    }

    /**
     * @return A axis-aligned bounding box that surrounds the skeletal mesh given its default pose.
     */
    public AABBf getStaticAABB() {
        return staticAABB;
    }

    private void calculateNormals() {
        // TODO: Better algorithm (take into account triangle size and angles
        // From the positions as loaded, which is where the skinning takes them from. The bind pose above composes the
        // bone transforms in the reverse order and transposed, and a normal taken from it was turned by the rest
        // rotation of its bone before the skinning turned it again.
        List<Vector3f> vertices = this.vertices;
        List<Vector3f> normals = IntStream.range(0, vertices.size()).mapToObj(i -> new Vector3f()).collect(Collectors.toCollection(() ->
                Lists.newArrayListWithCapacity(vertices.size())));
        Vector3f v1 = new Vector3f();
        Vector3f v2 = new Vector3f();
        Vector3f norm = new Vector3f();
        for (int i = 0; i < indices.size() / 3; ++i) {
            Vector3f baseVert = vertices.get(indices.get(i * 3));
            vertices.get(indices.get(i * 3 + 1)).sub(baseVert, v1);
            vertices.get(indices.get(i * 3 + 2)).sub(baseVert, v2);
            v1.normalize();
            v2.normalize();
            v2.cross(v1, norm);
            normals.get(indices.get(i * 3)).add(norm);
            normals.get(indices.get(i * 3 + 1)).add(norm);
            normals.get(indices.get(i * 3 + 2)).add(norm);
        }

        normals.forEach(Vector3f::normalize);

        this.normals = normals;
    }
}
