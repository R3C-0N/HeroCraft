// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.topology;

import org.terasology.context.annotation.API;

/**
 * A continuous position on the cube net: a face, and coordinates in blocks inside it.
 * <p>
 * The face is <em>carried</em>, never guessed back. Re-deriving it with a floor loses it on a
 * border: a position folded exactly onto the edge of a cell would name the cell next door, which
 * may well be on another face or on a dead slot.
 */
@API
public record FacePos(int face, double u, double v) {

    /** @return the net X of this position */
    public double netX(int faceSize) {
        return CubePattern.PATTERN[face][0] * (double) faceSize + u;
    }

    /** @return the net Y of this position */
    public double netY(int faceSize) {
        return CubePattern.PATTERN[face][1] * (double) faceSize + v;
    }

    /**
     * @return the position of a live net coordinate, or null when it falls on a dead slot or
     *         outside the net
     */
    public static FacePos of(double netX, double netY, int faceSize) {
        int face = CubePattern.faceOf(netX, netY, faceSize);
        if (face == CubePattern.NO_FACE) {
            return null;
        }
        return new FacePos(face,
                netX - CubePattern.PATTERN[face][0] * (double) faceSize,
                netY - CubePattern.PATTERN[face][1] * (double) faceSize);
    }
}
