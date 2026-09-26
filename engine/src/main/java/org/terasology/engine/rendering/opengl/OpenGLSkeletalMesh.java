// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.engine.rendering.opengl;

import org.joml.Matrix4f;
import org.joml.Vector2f;
import org.joml.Vector3f;
import org.lwjgl.BufferUtils;
import org.lwjgl.opengl.GL30;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.engine.core.GameThread;
import org.terasology.engine.core.subsystem.lwjgl.LwjglGraphicsProcessing;
import org.terasology.engine.rendering.assets.mesh.StandardMeshData;
import org.terasology.engine.rendering.assets.skeletalmesh.Bone;
import org.terasology.engine.rendering.assets.skeletalmesh.BoneWeight;
import org.terasology.engine.rendering.assets.skeletalmesh.SkeletalMesh;
import org.terasology.engine.rendering.assets.skeletalmesh.SkeletalMeshData;
import org.terasology.gestalt.assets.AssetType;
import org.terasology.gestalt.assets.DisposableResource;
import org.terasology.gestalt.assets.ResourceUrn;
import org.terasology.joml.geom.AABBf;

import java.nio.ByteBuffer;
import java.nio.FloatBuffer;
import java.nio.IntBuffer;
import java.util.Collection;
import java.util.List;

import static org.lwjgl.opengl.GL11.GL_UNSIGNED_INT;

/**
 * A skeletal mesh on the GPU.
 * <p>
 * The vertices are uploaded once, at rest and with the bones they follow, and the skinning is done by the vertex
 * shader from one matrix per bone (genericMeshMaterial_vert.glsl): a creature costs a uniform array per frame, not a
 * rebuilt vertex buffer. A mesh with more bones than the shader has room for is skinned on the CPU instead, into a
 * buffer of its own, which is how every mesh used to be drawn.
 */
public class OpenGLSkeletalMesh extends SkeletalMesh {
    /** Bones the shader has room for. Mirrored by MAX_BONES in genericMeshMaterial_vert.glsl. */
    public static final int MAX_BONES = 48;

    /** Attribute locations of the skinning, past the ones StandardMeshData uses. */
    public static final int JOINTS_INDEX = 6;
    public static final int WEIGHTS_INDEX = 7;

    private static final int POSITION_SIZE = 3 * Float.BYTES;
    private static final int NORMAL_SIZE = 3 * Float.BYTES;
    private static final int UV_SIZE = 2 * Float.BYTES;
    private static final int JOINTS_SIZE = SkeletalMeshData.MAX_WEIGHTS;
    private static final int WEIGHTS_SIZE = SkeletalMeshData.MAX_WEIGHTS * Float.BYTES;
    private static final int STRIDE = POSITION_SIZE + NORMAL_SIZE + UV_SIZE + JOINTS_SIZE + WEIGHTS_SIZE;
    private static final int CPU_STRIDE = POSITION_SIZE + NORMAL_SIZE;

    private static final Logger logger = LoggerFactory.getLogger(OpenGLSkeletalMesh.class);

    private SkeletalMeshData data;

    private Vector3f scale;
    private Vector3f translate;

    private DisposalAction disposalAction;

    public OpenGLSkeletalMesh(ResourceUrn urn, AssetType<?, SkeletalMeshData> assetType,
                              SkeletalMeshData data, LwjglGraphicsProcessing graphicsProcessing,
                              OpenGLSkeletalMesh.DisposalAction disposalAction) {
        super(urn, assetType, disposalAction);
        this.disposalAction = disposalAction;
        graphicsProcessing.asynchToDisplayThread(() -> {
            reload(data);
        });
    }

    public static OpenGLSkeletalMesh create(ResourceUrn urn, AssetType<?, SkeletalMeshData> assetType,
                                            SkeletalMeshData data, LwjglGraphicsProcessing graphicsProcessing) {
        return new OpenGLSkeletalMesh(urn, assetType, data, graphicsProcessing, new DisposalAction(urn));
    }


    public void setScaleTranslate(Vector3f newScale, Vector3f newTranslate) {
        this.scale = newScale;
        this.translate = newTranslate;
    }

    /**
     * @return whether the shader can skin this mesh, which it can when its bones fit in the uniform array
     */
    public boolean isSkinnedOnGpu() {
        return data != null && data.getBones().size() <= MAX_BONES;
    }

    @Override
    protected void doReload(SkeletalMeshData newData) {
        try {
            GameThread.synch(() -> {
                this.data = newData;

                if (this.disposalAction.vao == 0) {
                    this.disposalAction.vao = GL30.glGenVertexArrays();
                    this.disposalAction.vbo = GL30.glGenBuffers();
                    this.disposalAction.ebo = GL30.glGenBuffers();
                }
                GL30.glBindVertexArray(this.disposalAction.vao);
                GL30.glBindBuffer(GL30.GL_ARRAY_BUFFER, this.disposalAction.vbo);
                GL30.glBufferData(GL30.GL_ARRAY_BUFFER, restVertices(newData), GL30.GL_STATIC_DRAW);

                int offset = 0;
                GL30.glEnableVertexAttribArray(StandardMeshData.VERTEX_INDEX);
                GL30.glVertexAttribPointer(StandardMeshData.VERTEX_INDEX, 3, GL30.GL_FLOAT, false, STRIDE, offset);
                offset += POSITION_SIZE;
                GL30.glEnableVertexAttribArray(StandardMeshData.NORMAL_INDEX);
                GL30.glVertexAttribPointer(StandardMeshData.NORMAL_INDEX, 3, GL30.GL_FLOAT, false, STRIDE, offset);
                offset += NORMAL_SIZE;
                GL30.glEnableVertexAttribArray(StandardMeshData.UV0_INDEX);
                GL30.glVertexAttribPointer(StandardMeshData.UV0_INDEX, 2, GL30.GL_FLOAT, false, STRIDE, offset);
                offset += UV_SIZE;
                GL30.glEnableVertexAttribArray(JOINTS_INDEX);
                GL30.glVertexAttribIPointer(JOINTS_INDEX, SkeletalMeshData.MAX_WEIGHTS, GL30.GL_UNSIGNED_BYTE, STRIDE, offset);
                offset += JOINTS_SIZE;
                GL30.glEnableVertexAttribArray(WEIGHTS_INDEX);
                GL30.glVertexAttribPointer(WEIGHTS_INDEX, SkeletalMeshData.MAX_WEIGHTS, GL30.GL_FLOAT, false, STRIDE, offset);

                IntBuffer indexBuffer = BufferUtils.createIntBuffer(newData.getIndices().size());
                indexBuffer.put(newData.getIndices().toArray());
                indexBuffer.flip();
                GL30.glBindBuffer(GL30.GL_ELEMENT_ARRAY_BUFFER, this.disposalAction.ebo);
                GL30.glBufferData(GL30.GL_ELEMENT_ARRAY_BUFFER, indexBuffer, GL30.GL_STATIC_DRAW);

                GL30.glBindVertexArray(0);

                if (this.disposalAction.cpuVao != 0) {
                    this.disposalAction.disposeCpu();
                }
                if (!isSkinnedOnGpu()) {
                    logger.warn("{} has {} bones, more than the {} the shader skins: it is skinned on the CPU", //NOPMD
                            getUrn(), newData.getBones().size(), MAX_BONES);
                }
            });
        } catch (InterruptedException e) {
            logger.error("Failed to reload {}", getUrn(), e); //NOPMD
        }
    }

    /** One interleaved vertex per entry: position, normal, uv, then the four bones it follows and their weights. */
    private static ByteBuffer restVertices(SkeletalMeshData data) {
        List<Vector3f> positions = data.getRestPositions();
        List<Vector3f> normals = data.getRestNormals();
        List<Vector2f> uvs = data.getUVs();
        List<BoneWeight> weights = data.getWeights();
        ByteBuffer buffer = BufferUtils.createByteBuffer(STRIDE * data.getVertexCount());
        for (int i = 0; i < data.getVertexCount(); i++) {
            Vector3f p = positions.get(i);
            buffer.putFloat(p.x).putFloat(p.y).putFloat(p.z);
            Vector3f n = normals.get(i);
            buffer.putFloat(n.x).putFloat(n.y).putFloat(n.z);
            Vector2f uv = i < uvs.size() ? uvs.get(i) : new Vector2f();
            buffer.putFloat(uv.x).putFloat(uv.y);
            BoneWeight weight = weights.get(i);
            int count = Math.min(weight.jointCount(), SkeletalMeshData.MAX_WEIGHTS);
            for (int w = 0; w < SkeletalMeshData.MAX_WEIGHTS; w++) {
                // A joint past MAX_BONES is only ever read on the CPU path, which takes it from the data itself.
                buffer.put((byte) (w < count ? Math.min(weight.getJoint(w), 255) : 0));
            }
            for (int w = 0; w < SkeletalMeshData.MAX_WEIGHTS; w++) {
                buffer.putFloat(w < count ? weight.getBias(w) : 0f);
            }
        }
        buffer.flip();
        return buffer;
    }

    /**
     * Draws the mesh as the shader skins it: the caller has handed the material the bone matrices, and their count.
     * With no bones set, the mesh is drawn at rest.
     */
    public void render() {
        GL30.glBindVertexArray(disposalAction.vao);
        GL30.glDrawElements(GL30.GL_TRIANGLES, data.getIndices().size(), GL_UNSIGNED_INT, 0);
        GL30.glBindVertexArray(0);
    }

    /**
     * Skins the mesh on the CPU and draws it, for a mesh with more bones than the shader holds; the material must be
     * told there are no bones to apply, or they would be applied twice.
     *
     * @param boneTransforms one matrix per bone, transposed, as {@link SkeletalMeshData#getVertexPositions} takes them
     */
    public void render(List<Matrix4f> boneTransforms) {
        List<Vector3f> verts = data.getVertexPositions(boneTransforms);
        List<Vector3f> normals = data.getVertexNormals(boneTransforms);
        if (disposalAction.cpuVao == 0) {
            createCpuBuffers();
        }
        GL30.glBindVertexArray(disposalAction.cpuVao);

        FloatBuffer vertBuffer = BufferUtils.createFloatBuffer(verts.size() * CPU_STRIDE / Float.BYTES);
        for (int i = 0; i < verts.size(); ++i) {
            Vector3f vert = verts.get(i);
            vertBuffer.put(vert.x * scale.x + translate.x);
            vertBuffer.put(vert.y * scale.y + translate.y);
            vertBuffer.put(vert.z * scale.z + translate.z);
            Vector3f norm = normals.get(i);
            vertBuffer.put(norm.x);
            vertBuffer.put(norm.y);
            vertBuffer.put(norm.z);
        }
        vertBuffer.flip();
        GL30.glBindBuffer(GL30.GL_ARRAY_BUFFER, this.disposalAction.cpuVbo);
        GL30.glBufferSubData(GL30.GL_ARRAY_BUFFER, 0, vertBuffer);

        GL30.glDrawElements(GL30.GL_TRIANGLES, data.getIndices().size(), GL_UNSIGNED_INT, 0);
        GL30.glBindVertexArray(0);
    }

    /** Positions and normals from a buffer rewritten every draw; the uvs and the indices are the static ones. */
    private void createCpuBuffers() {
        disposalAction.cpuVao = GL30.glGenVertexArrays();
        disposalAction.cpuVbo = GL30.glGenBuffers();
        GL30.glBindVertexArray(disposalAction.cpuVao);
        GL30.glBindBuffer(GL30.GL_ARRAY_BUFFER, disposalAction.cpuVbo);
        GL30.glBufferData(GL30.GL_ARRAY_BUFFER, (long) CPU_STRIDE * data.getVertexCount(), GL30.GL_STREAM_DRAW);
        GL30.glEnableVertexAttribArray(StandardMeshData.VERTEX_INDEX);
        GL30.glVertexAttribPointer(StandardMeshData.VERTEX_INDEX, 3, GL30.GL_FLOAT, false, CPU_STRIDE, 0);
        GL30.glEnableVertexAttribArray(StandardMeshData.NORMAL_INDEX);
        GL30.glVertexAttribPointer(StandardMeshData.NORMAL_INDEX, 3, GL30.GL_FLOAT, false, CPU_STRIDE, POSITION_SIZE);
        GL30.glBindBuffer(GL30.GL_ARRAY_BUFFER, disposalAction.vbo);
        GL30.glEnableVertexAttribArray(StandardMeshData.UV0_INDEX);
        GL30.glVertexAttribPointer(StandardMeshData.UV0_INDEX, 2, GL30.GL_FLOAT, false, STRIDE, POSITION_SIZE + NORMAL_SIZE);
        GL30.glBindBuffer(GL30.GL_ELEMENT_ARRAY_BUFFER, disposalAction.ebo);
        GL30.glBindVertexArray(0);
    }

    @Override
    public int getVertexCount() {
        return data.getVertexCount();
    }

    @Override
    public Collection<Bone> getBones() {
        return data.getBones();
    }

    /**
     * @return all bones, each one after its parent
     */
    public List<Bone> getBonesParentFirst() {
        return data.getBonesParentFirst();
    }

    /**
     * @see SkeletalMeshData#skinMatrices
     */
    public void skinMatrices(SkeletalMeshData.BoneLocal localOf, Matrix4f[] model, Matrix4f[] skin) {
        data.skinMatrices(localOf, model, skin);
    }

    @Override
    public Bone getBone(String boneName) {
        return data.getBone(boneName);
    }

    @Override
    public AABBf getStaticAabb() {
        return data.getStaticAABB();
    }

    private static class DisposalAction implements DisposableResource {

        private final ResourceUrn urn;

        private int vao = 0;
        private int vbo = 0;
        private int ebo = 0;
        private int cpuVao = 0;
        private int cpuVbo = 0;

        DisposalAction(ResourceUrn urn) {
            this.urn = urn;
        }

        public void dispose() {
            if (vao != 0) {
                GL30.glDeleteVertexArrays(vao);
            }
            if (vbo != 0) {
                GL30.glDeleteBuffers(vbo);
            }
            if (ebo != 0) {
                GL30.glDeleteBuffers(ebo);
            }
            disposeCpu();
            vao = 0;
            vbo = 0;
            ebo = 0;
        }

        void disposeCpu() {
            if (cpuVao != 0) {
                GL30.glDeleteVertexArrays(cpuVao);
            }
            if (cpuVbo != 0) {
                GL30.glDeleteBuffers(cpuVbo);
            }
            cpuVao = 0;
            cpuVbo = 0;
        }

        @Override
        public void close() {
            try {
                GameThread.synch(() -> {
                    dispose();
                });
            } catch (InterruptedException e) {
                logger.error("Failed to dispose {}", urn, e);
            }
        }
    }
}
