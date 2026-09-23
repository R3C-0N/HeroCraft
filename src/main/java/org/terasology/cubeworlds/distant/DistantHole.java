// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.lwjgl.BufferUtils;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL30;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.chunks.Chunk;

import java.nio.ByteBuffer;

/**
 * Where the game already draws the world, one texel per column of chunks of the net.
 * <p>
 * The distant terrain leaves a hole exactly there, and nowhere else: not a disc of some radius,
 * which would overlap the loaded box in its corners and leave a gap along its sides, and not a box
 * either, which knows nothing of the chunks still on their way or of the ones a cut seam puts a
 * whole face away in the grid. A column counts as drawn when one of its chunks has geometry on the
 * GPU, in any phase — counting only the opaque one left the sea out, and the distance was drawn
 * under the real water, hiding its bed — and when none of its loaded chunks is still waiting for its
 * mesh. Without that second rule a column whose deep chunk was meshed and whose surface chunk was
 * not yet counted as drawn, and showed the sky: along the edge of the loaded box that was a ring a
 * chunk wide, a bright line two pixels thick seen from afar, and grey patches under a pilot. A pilot
 * high enough for the ground chunks to unload gets the distance back underneath, and a hole never
 * opens on nothing.
 */
public final class DistantHole {

    private final int side;
    private final byte[] marks;
    private final ByteBuffer upload;
    private boolean dirty = true;
    private int textureId;

    public DistantHole(CubeGeometry geometry) {
        this.side = CubePattern.PATTERN_SIDE * geometry.faceChunks();
        this.marks = new byte[side * side];
        this.upload = BufferUtils.createByteBuffer(side * side);
    }

    /** Recounts the drawn columns. Main thread: it reads the chunks' meshes. */
    public void mark(Iterable<Chunk> chunks) {
        byte[] next = new byte[marks.length];
        // The lowest and highest chunk loaded in each column: those two are never meshed, having no
        // neighbour on one side, and waiting for them would wait for ever.
        int[] lowest = new int[marks.length];
        int[] highest = new int[marks.length];
        java.util.Arrays.fill(lowest, Integer.MAX_VALUE);
        java.util.Arrays.fill(highest, Integer.MIN_VALUE);
        for (Chunk chunk : chunks) {
            int i = column(chunk);
            if (i >= 0) {
                lowest[i] = Math.min(lowest[i], chunk.getPosition().y());
                highest[i] = Math.max(highest[i], chunk.getPosition().y());
            }
        }
        boolean[] waiting = new boolean[marks.length];
        for (Chunk chunk : chunks) {
            int i = column(chunk);
            if (i < 0) {
                continue;
            }
            if (!chunk.isReady() || !chunk.hasMesh()) {
                int y = chunk.getPosition().y();
                waiting[i] |= y > lowest[i] && y < highest[i];
                continue;
            }
            ChunkMesh mesh = chunk.getMesh();
            if (mesh.triangleCount(ChunkMesh.RenderPhase.OPAQUE) == 0
                    && mesh.triangleCount(ChunkMesh.RenderPhase.ALPHA_REJECT) == 0
                    && mesh.triangleCount(ChunkMesh.RenderPhase.REFRACTIVE) == 0) {
                continue;
            }
            next[i] = (byte) 0xFF;
        }
        for (int i = 0; i < next.length; i++) {
            if (waiting[i]) {
                next[i] = 0;
            }
        }
        if (!java.util.Arrays.equals(next, marks)) {
            System.arraycopy(next, 0, marks, 0, marks.length);
            dirty = true;
        }
    }

    /** The texture, uploaded again if the marks moved. Main thread, with the GL context current. */
    public int textureId() {
        if (textureId == 0) {
            textureId = GL11.glGenTextures();
            GL11.glBindTexture(GL11.GL_TEXTURE_2D, textureId);
            GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL11.GL_TEXTURE_MIN_FILTER, GL11.GL_NEAREST);
            GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL11.GL_TEXTURE_MAG_FILTER, GL11.GL_NEAREST);
            GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL30.GL_TEXTURE_MAX_LEVEL, 0);
        }
        if (dirty) {
            upload.clear();
            upload.put(marks).flip();
            GL11.glBindTexture(GL11.GL_TEXTURE_2D, textureId);
            GL11.glPixelStorei(GL11.GL_UNPACK_ALIGNMENT, 1);
            GL11.glTexImage2D(GL11.GL_TEXTURE_2D, 0, GL30.GL_R8, side, side, 0, GL11.GL_RED,
                    GL11.GL_UNSIGNED_BYTE, upload);
            GL11.glPixelStorei(GL11.GL_UNPACK_ALIGNMENT, 4);
            dirty = false;
        }
        return textureId;
    }

    /**
     * Whether the game draws every column of chunks under a square of the net: a node standing
     * wholly in the hole is not sent to the GPU at all, rather than shaded and thrown away.
     */
    /** The texel of a chunk's column, or -1 off the net. */
    private int column(Chunk chunk) {
        int x = chunk.getPosition().x();
        int z = chunk.getPosition().z();
        return x < 0 || z < 0 || x >= side || z >= side ? -1 : x + z * side;
    }

    public boolean covers(int netX, int netZ, int span) {
        int x0 = Math.floorDiv(netX, 32);
        int z0 = Math.floorDiv(netZ, 32);
        int x1 = Math.floorDiv(netX + span - 1, 32);
        int z1 = Math.floorDiv(netZ + span - 1, 32);
        for (int z = z0; z <= z1; z++) {
            for (int x = x0; x <= x1; x++) {
                if (x < 0 || z < 0 || x >= side || z >= side || marks[x + z * side] == 0) {
                    return false;
                }
            }
        }
        return true;
    }

    /** How many columns of chunks are drawn by the game, for the bench. */
    public int drawnColumns() {
        int n = 0;
        for (byte b : marks) {
            if (b != 0) {
                n++;
            }
        }
        return n;
    }

    public void dispose() {
        if (textureId != 0) {
            GL11.glDeleteTextures(textureId);
            textureId = 0;
        }
    }
}
