// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.topology;

import org.terasology.context.annotation.API;

/**
 * The cube net: six square faces laid out as a cross inside a 4x4 grid of face slots.
 * <p>
 * The net lives in the world's XZ plane; Y is the depth axis, local to each face. A face is
 * therefore an ordinary flat heightmap whose up direction is +Y, which is why the stock elevation,
 * density and sunlight machinery keeps working unchanged.
 * <pre>
 *   row 2      .      [ +Z ]     .        .      north cap
 *   row 1   [ +X ]  [ +Y ]   [ -X ]   [ -Y ]     equatorial band
 *   row 0      .      [ -Z ]     .        .      south cap
 *   row 3      .        .        .        .      dead
 * </pre>
 * Six of the sixteen slots carry world; the other ten are dead and generate air. That waste buys a
 * single definition of the world's shape: the net is a rectangle in memory, so chunk keys, flat
 * indexing, persistence and network messages are untouched.
 */
@API
public final class CubePattern {

    /** Side of the slot grid, in faces. */
    public static final int PATTERN_SIDE = 4;

    /** Number of live faces. */
    public static final int FACES = 6;

    /** Slot (column, row) of each face, row 0 at the bottom. Index is the face id. */
    public static final int[][] PATTERN = {
            {0, 1}, {1, 1}, {2, 1}, {3, 1}, {1, 2}, {1, 0}
    };

    /** Human-readable face names, indexed by face id. */
    public static final String[] NAMES = {"+X", "+Y", "-X", "-Y", "+Z north", "-Z south"};

    /** Face id of the north polar cap. */
    public static final int NORTH = 4;

    /** Face id of the south polar cap. */
    public static final int SOUTH = 5;

    /** Returned by every lookup that lands on a dead slot. */
    public static final int NO_FACE = -1;

    private CubePattern() {
    }

    /**
     * @return the face occupying the given slot, or {@link #NO_FACE} if that slot is dead
     */
    public static int faceAt(int column, int row) {
        for (int face = 0; face < FACES; face++) {
            if (PATTERN[face][0] == column && PATTERN[face][1] == row) {
                return face;
            }
        }
        return NO_FACE;
    }

    /**
     * @return the face containing the given net coordinate, or {@link #NO_FACE} when it falls
     *         outside the net or onto a dead slot
     */
    public static int faceOf(int netX, int netY, int faceSize) {
        if (netX < 0 || netY < 0 || netX >= PATTERN_SIDE * faceSize || netY >= PATTERN_SIDE * faceSize) {
            return NO_FACE;
        }
        return faceAt(netX / faceSize, netY / faceSize);
    }

    /** Same as {@link #faceOf(int, int, int)} for a continuous coordinate. */
    public static int faceOf(double netX, double netY, int faceSize) {
        return faceOf((int) Math.floor(netX), (int) Math.floor(netY), faceSize);
    }

    /** @return the net coordinate of local column {@code u} on {@code face} */
    public static int netX(int face, int u, int faceSize) {
        return PATTERN[face][0] * faceSize + u;
    }

    /** @return the net coordinate of local row {@code v} on {@code face} */
    public static int netY(int face, int v, int faceSize) {
        return PATTERN[face][1] * faceSize + v;
    }

    /** @return the local column of a net coordinate known to sit on {@code face} */
    public static int localU(int face, int netX, int faceSize) {
        return netX - PATTERN[face][0] * faceSize;
    }

    /** @return the local row of a net coordinate known to sit on {@code face} */
    public static int localV(int face, int netY, int faceSize) {
        return netY - PATTERN[face][1] * faceSize;
    }

    /** @return true when the given net coordinate carries world rather than air */
    public static boolean isAlive(int netX, int netY, int faceSize) {
        return faceOf(netX, netY, faceSize) != NO_FACE;
    }
}
