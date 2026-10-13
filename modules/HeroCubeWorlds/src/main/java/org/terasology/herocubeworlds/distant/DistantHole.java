// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.lwjgl.BufferUtils;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL30;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.chunks.Chunks;

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
 * high enough for the ground chunks to unload gets the distance back underneath.
 * <p>
 * Round the player the hole is kept open whatever is drawn ({@link #mark(Iterable, int, int, int)}):
 * a ring of chunks where the distance never stands in, so that after a teleport or a fast flight the
 * first thing seen under one's feet is the sky while the chunks come, never a coarse block of ground
 * eight wide and at the wrong height. The loading screen waits for that same ring. The ring is a
 * square of the grid, so it stops at a cut seam: across it, the distance still stands in until the
 * chunks there are drawn. Its texels are {@link #KEPT}, the chunks' {@link #DRAWN}: the shaders cut
 * both alike, at a half, and only the count tells them apart.
 * <p>
 * A drawn column that touches an open one is {@link #EDGE}: there the two terrains are crossfaded
 * rather than cut, each pixel the chunks give way on taken by the distance, more of them nearer the
 * border — the transition Distant Horizons makes, which hides the line the eye otherwise finds.
 * The distance is therefore sent for those columns too. A slot of the net no face covers is
 * {@link #DEAD}, so that a cut seam is never taken for an open column and faded.
 */
public final class DistantHole {

    /** A column the chunks draw. */
    static final byte DRAWN = (byte) 0xFF;
    /** A column only the ring round the player keeps clear: nothing is drawn there yet. */
    static final byte KEPT = (byte) 0xC0;
    /** A column the chunks draw, next to one they do not: the shaders crossfade it. */
    static final byte EDGE = (byte) 0xE0;
    /** A slot of the net no face covers: never open, never faded into. */
    static final byte DEAD = (byte) 0x30;

    private final CubeGeometry geometry;
    private final int side;
    private final byte[] marks;
    private final boolean[] dead;
    private final ByteBuffer upload;
    private boolean dirty = true;
    private int textureId;

    public DistantHole(CubeGeometry geometry) {
        this.geometry = geometry;
        this.side = CubePattern.PATTERN_SIDE * geometry.faceChunks();
        this.marks = new byte[side * side];
        this.dead = new boolean[side * side];
        for (int z = 0; z < side; z++) {
            for (int x = 0; x < side; x++) {
                dead[x + z * side] = geometry.faceOf(x * Chunks.SIZE_X + Chunks.SIZE_X / 2,
                        z * Chunks.SIZE_Z + Chunks.SIZE_Z / 2) == CubePattern.NO_FACE;
            }
        }
        this.upload = BufferUtils.createByteBuffer(side * side);
    }

    /**
     * Recounts the drawn columns, and keeps clear those within {@code radius} chunks of the given
     * column; a negative radius keeps none. Main thread: it reads the chunks' meshes.
     */
    public void mark(Iterable<Chunk> chunks, int centreX, int centreZ, int radius) {
        mark(chunks, centreX, centreZ, radius, null);
    }

    /**
     * As {@link #mark(Iterable, int, int, int)}, crossfading a drawn column into the distance only where
     * the distance is there to take over: a column {@code distant} does not mark keeps its chunks whole,
     * rather than showing the sky through them while its node builds. Null for everywhere.
     */
    public void mark(Iterable<Chunk> chunks, int centreX, int centreZ, int radius, boolean[] distant) {
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
            next[i] = DRAWN;
        }
        for (int i = 0; i < next.length; i++) {
            if (waiting[i]) {
                next[i] = 0;
            }
        }
        for (int z = centreZ - radius; z <= centreZ + radius; z++) {
            for (int x = centreX - radius; x <= centreX + radius; x++) {
                if (x >= 0 && z >= 0 && x < side && z < side && next[x + z * side] == 0 && !dead[x + z * side]) {
                    next[x + z * side] = KEPT;
                }
            }
        }
        for (int i = 0; i < next.length; i++) {
            if (next[i] == 0 && dead[i]) {
                next[i] = DEAD;
            }
        }
        for (int z = 0; z < side; z++) {
            for (int x = 0; x < side; x++) {
                if (next[x + z * side] == DRAWN && (distant == null || distant[x + z * side])
                        && touchesOpen(next, x, z)) {
                    next[x + z * side] = EDGE;
                }
            }
        }
        if (!java.util.Arrays.equals(next, marks)) {
            System.arraycopy(next, 0, marks, 0, marks.length);
            dirty = true;
        }
    }

    /** Whether a column has an open one among its eight neighbours, where the distance shows. */
    private boolean touchesOpen(byte[] next, int x, int z) {
        for (int dz = -1; dz <= 1; dz++) {
            for (int dx = -1; dx <= 1; dx++) {
                int nx = x + dx;
                int nz = z + dz;
                if (nx >= 0 && nz >= 0 && nx < side && nz < side && next[nx + nz * side] == 0) {
                    return true;
                }
            }
        }
        return false;
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

    /** Columns of chunks along a side of the net, the grid the marks are kept on. */
    public int side() {
        return side;
    }

    /** The texel of a chunk's column, or -1 off the net. */
    private int column(Chunk chunk) {
        int x = chunk.getPosition().x();
        int z = chunk.getPosition().z();
        return x < 0 || z < 0 || x >= side || z >= side ? -1 : x + z * side;
    }

    /**
     * Whether the hole is open under every column of chunks of a square of the net: a node standing
     * wholly in it is not sent to the GPU at all, rather than shaded and thrown away.
     */
    public boolean covers(int netX, int netZ, int span) {
        int x0 = Math.floorDiv(netX, 32);
        int z0 = Math.floorDiv(netZ, 32);
        int x1 = Math.floorDiv(netX + span - 1, 32);
        int z1 = Math.floorDiv(netZ + span - 1, 32);
        for (int z = z0; z <= z1; z++) {
            for (int x = x0; x <= x1; x++) {
                byte mark = x < 0 || z < 0 || x >= side || z >= side ? 0 : marks[x + z * side];
                if (mark == 0 || mark == EDGE) {
                    return false;
                }
            }
        }
        return true;
    }

    /** How many columns of chunks are drawn by the game, for the bench. */
    public int drawnColumns() {
        return count(DRAWN) + count(EDGE);
    }

    /** How many drawn columns border the distance and are crossfaded into it, for the bench. */
    public int edgeColumns() {
        return count(EDGE);
    }

    /** How many columns only the ring round the player keeps clear, for the bench. */
    public int keptColumns() {
        return count(KEPT);
    }

    private int count(byte mark) {
        int n = 0;
        for (byte b : marks) {
            if (b == mark) {
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
