// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.topology;

import org.terasology.context.annotation.API;

/**
 * Folding: turning coordinates that have run off the side of a face into the face next door.
 * <p>
 * Every read of the world goes through here. A sampler that clamps at the edge of a face instead
 * gets a flat, truncated neighbourhood, and the seam shows up as a crease that no fixed screenshot
 * will reveal.
 */
@API
public final class CubeFolding {

    /** Guards against a runaway walk; a legitimate fold never needs more than a handful. */
    private static final int MAX_STEPS = 64;

    private CubeFolding() {
    }

    /**
     * Folds integer local coordinates back onto a real face.
     * <p>
     * Resolves {@code u} before {@code v}, so the step must stay shorter than one cell: a longer
     * one can cross the prolongation of an edge, which corresponds to nothing on the cube. Near a
     * corner the map is not injective — three faces offer 270 degrees where the unfolded plane
     * offers 360 — and the surplus quarter falls back onto world already seen. That is accepted.
     *
     * @param dest receives {face, u, v, quarterTurns}
     */
    public static void foldRaw(int face, int u, int v, int size, int[] dest) {
        FaceBasis basis = FaceBasis.BASES[face];
        int lu = u;
        int lv = v;
        int[] rotated = new int[2];
        for (int step = 0; step < MAX_STEPS; step++) {
            if (lu >= size) {
                lu -= size;
                basis = basis.acrossRightPlus();
            } else if (lu < 0) {
                lu += size;
                basis = basis.acrossRightMinus();
            } else if (lv >= size) {
                lv -= size;
                basis = basis.acrossUpPlus();
            } else if (lv < 0) {
                lv += size;
                basis = basis.acrossUpMinus();
            } else {
                int k = basis.quarterTurns();
                FaceBasis.rotate(lu, lv, k, size, rotated);
                dest[0] = basis.face();
                dest[1] = rotated[0];
                dest[2] = rotated[1];
                dest[3] = k;
                return;
            }
        }
        throw new IllegalArgumentException("coordinates too far off the face to fold: " + u + ", " + v);
    }

    /**
     * Folds a continuous position. The fractional part is rotated about the centre of its cell,
     * which is the only place in the whole scheme where a fraction turns.
     */
    public static FacePos fold(int face, double u, double v, int size) {
        int cellU = (int) Math.floor(u);
        int cellV = (int) Math.floor(v);
        int[] folded = new int[4];
        foldRaw(face, cellU, cellV, size, folded);

        double fu = u - cellU - 0.5;
        double fv = v - cellV - 0.5;
        int cos = FaceBasis.COS_SIN[folded[3]][0];
        int sin = FaceBasis.COS_SIN[folded[3]][1];
        return new FacePos(folded[0],
                folded[1] + 0.5 + (fu * cos - fv * sin),
                folded[2] + 0.5 + (fu * sin + fv * cos));
    }

    /**
     * The eight directions, in the order that decides who wins at a corner.
     */
    public static final int[][] NEIGHBOUR_DELTA = {
            {-1, -1}, {0, -1}, {1, -1}, {-1, 0}, {1, 0}, {-1, 1}, {0, 1}, {1, 1}
    };

    /**
     * @return the net coordinates of a neighbouring cell, or null when the step lands nowhere.
     *         Never returns a dead slot: folding sends the step onto the face next door instead.
     */
    public static int[] neighbour(int netX, int netY, int dx, int dy, int faceSize) {
        int face = CubePattern.faceOf(netX, netY, faceSize);
        if (face == CubePattern.NO_FACE) {
            return null;
        }
        int u = CubePattern.localU(face, netX, faceSize) + dx;
        int v = CubePattern.localV(face, netY, faceSize) + dy;
        int[] folded = new int[4];
        foldRaw(face, u, v, faceSize, folded);
        return new int[]{
                CubePattern.netX(folded[0], folded[1], faceSize),
                CubePattern.netY(folded[0], folded[2], faceSize)
        };
    }

    /**
     * The neighbourhood of a cell, deduplicated.
     * <p>
     * At the twenty-four cells that sit on a corner of a face, two of the eight directions lead to
     * the same cell, because three faces share that point. The first direction in
     * {@link #NEIGHBOUR_DELTA} wins, and such a cell ends up with seven neighbours rather than
     * eight. This is the single place where the topology of the graph is decided; anything that
     * walks the world must come through here or it will disagree.
     *
     * @return net coordinate pairs, at most eight of them
     */
    public static int[][] neighbours(int netX, int netY, int faceSize) {
        int[][] found = new int[NEIGHBOUR_DELTA.length][];
        int count = 0;
        for (int[] delta : NEIGHBOUR_DELTA) {
            int[] candidate = neighbour(netX, netY, delta[0], delta[1], faceSize);
            if (candidate == null) {
                continue;
            }
            boolean seen = false;
            for (int k = 0; k < count; k++) {
                if (found[k][0] == candidate[0] && found[k][1] == candidate[1]) {
                    seen = true;
                    break;
                }
            }
            if (!seen) {
                found[count++] = candidate;
            }
        }
        int[][] result = new int[count][];
        System.arraycopy(found, 0, result, 0, count);
        return result;
    }

    /**
     * Folds a net coordinate that may have run off its face, given the face it started from.
     * <p>
     * This is the entry point for a facet whose border spills past a seam: the border is a box in
     * net space, so some of its cells belong to another face and must be folded rather than read
     * where they lie.
     */
    public static FacePos foldNet(int fromFace, double netX, double netY, int faceSize) {
        double u = netX - CubePattern.PATTERN[fromFace][0] * (double) faceSize;
        double v = netY - CubePattern.PATTERN[fromFace][1] * (double) faceSize;
        return fold(fromFace, u, v, faceSize);
    }
}
