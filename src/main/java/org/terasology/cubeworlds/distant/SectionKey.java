// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.topology.CubePattern;

/**
 * A node of the quadtree of one face: which face, which level, which square.
 * <p>
 * One tree per face, because a quadtree is a rectangle and the net is not; the six trees meet
 * through the folding of the cube, never through a difference of coordinates. The root of each is
 * the whole face at the coarsest level, and every node splits into exactly four children on its own
 * face — the corners of the cube touch the neighbours of a node, never its children.
 * <p>
 * A node at level {@code L} holds {@link #columns} columns along a side, each covering {@code 2^L}
 * blocks: one column per block at level 0, and the whole face in one node at the top.
 * <p>
 * <b>A class and not a record</b>, and it has to stay one: a record's {@code equals} and
 * {@code hashCode} go through {@code java.lang.runtime.ObjectMethods}, which the module sandbox
 * refuses, so a record used as a key throws on the first lookup in the game — and never in a test,
 * which runs without the sandbox.
 */
public final class SectionKey {

    /** The coarsest level: the whole face in a single node. */
    public static final int TOP_LEVEL = 6;

    private final int face;
    private final int level;
    private final int i;
    private final int j;

    /**
     * @param face the face, as {@link CubePattern} numbers them
     * @param level 0 for one column per block, up to {@link #TOP_LEVEL}
     * @param i the node along the face's u axis, from 0
     * @param j the node along the face's v axis, from 0
     */
    public SectionKey(int face, int level, int i, int j) {
        this.face = face;
        this.level = level;
        this.i = i;
        this.j = j;
    }

    public int face() {
        return face;
    }

    public int level() {
        return level;
    }

    public int i() {
        return i;
    }

    public int j() {
        return j;
    }

    @Override
    public boolean equals(Object o) {
        if (!(o instanceof SectionKey)) {
            return false;
        }
        SectionKey k = (SectionKey) o;
        return face == k.face && level == k.level && i == k.i && j == k.j;
    }

    @Override
    public int hashCode() {
        return ((face * 8 + level) * 4099 + i) * 4099 + j;
    }

    @Override
    public String toString() {
        return "SectionKey[face=" + face + ", level=" + level + ", i=" + i + ", j=" + j + "]";
    }

    /**
     * Columns along a side of every node, at every level. The face edge over two to the top level,
     * so it has to divide: 3200 blocks give 50, and a face size that does not divide by 64 is
     * refused rather than rounded.
     */
    public static int columns(CubeGeometry geometry) {
        int faceBlocks = geometry.faceBlocks();
        if (faceBlocks % (1 << TOP_LEVEL) != 0) {
            throw new IllegalArgumentException("a face of " + faceBlocks + " blocks does not split into "
                    + (1 << TOP_LEVEL) + " at the top level");
        }
        return faceBlocks >> TOP_LEVEL;
    }

    /** Nodes along a side of the face at a level. */
    public static int nodesPerSide(int level) {
        return 1 << (TOP_LEVEL - level);
    }

    /** Blocks covered by one column. */
    public int stride() {
        return 1 << level;
    }

    /** Blocks covered along a side of the node. */
    public int span(CubeGeometry geometry) {
        return columns(geometry) * stride();
    }

    /** The net column of the node's first column, as {x, z}. */
    public int[] netOrigin(CubeGeometry geometry) {
        int span = span(geometry);
        return new int[]{
                CubePattern.netX(face, i * span, geometry.faceBlocks()),
                CubePattern.netY(face, j * span, geometry.faceBlocks())
        };
    }

    /** The four nodes one level down. There are always four: they never leave the face. */
    public SectionKey[] children() {
        if (level == 0) {
            return new SectionKey[0];
        }
        int l = level - 1;
        return new SectionKey[]{
                new SectionKey(face, l, 2 * i, 2 * j),
                new SectionKey(face, l, 2 * i + 1, 2 * j),
                new SectionKey(face, l, 2 * i, 2 * j + 1),
                new SectionKey(face, l, 2 * i + 1, 2 * j + 1)
        };
    }
}
