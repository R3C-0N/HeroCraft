// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator;

/**
 * The plan of a ruin, as a pure function of the offset from its centre and of its seed.
 * <p>
 * A square court {@link #RADIUS} blocks from the centre to the wall: a broken enclosure with one
 * gap to walk in by, four pillars of unequal height, a little rubble, and an altar in the middle
 * carrying the anchor stone. The anchor stone is where the past will be opened, in lot 2; here it
 * only marks the spot.
 * <p>
 * <b>Pure, and that is what lets it cross chunks.</b> A ruin lands on up to four chunks, generated
 * in any order on any thread; each asks this function about its own blocks and gets the same answer
 * the others would. Nothing is remembered between chunks, so nothing can disagree.
 * <p>
 * The floor is laid at {@code dy = 0}, on the ground line. Under it a foundation fills what is air
 * or water down to {@link #FOUNDATION_DEPTH}, so a ruin on a gentle slope stands on a plinth rather
 * than on stilts; over it the court is cleared up to {@link #CLEAR_HEIGHT}, which cuts the platform
 * into the hillside and takes out any tree that grew there.
 */
public final class RuinShape {

    /** From the centre to the wall, in blocks. */
    public static final int RADIUS = 7;

    /** How deep the foundation may fill under the floor. */
    public static final int FOUNDATION_DEPTH = 6;

    /** How high the court is cleared over the floor: more than a tree of the stock chain. */
    public static final int CLEAR_HEIGHT = 20;

    /** Where the pillars stand, on both axes. */
    public static final int PILLAR_OFFSET = 4;

    /** Half the width of the entrance. */
    private static final int GATE = 1;

    public enum Part {
        /** Not part of the ruin: leave the block alone. */
        NONE,
        /** Air: the court and the space above it. */
        CLEAR,
        FLOOR,
        /** Fill under the floor, only where there was air or water. */
        FOUNDATION,
        WALL,
        PILLAR,
        RUBBLE,
        ALTAR,
        ANCHOR
    }

    private RuinShape() {
    }

    /** Where the anchor stone stands, above the centre of the floor. */
    public static int anchorHeight() {
        return 2;
    }

    /** Which side the entrance opens on: 0 east, 1 west, 2 south, 3 north. */
    public static int gateSide(long seed) {
        return (int) (seed & 3);
    }

    public static Part part(int dx, int dy, int dz, long seed) {
        int ax = Math.abs(dx);
        int az = Math.abs(dz);
        if (ax > RADIUS || az > RADIUS || dy < -FOUNDATION_DEPTH || dy > CLEAR_HEIGHT) {
            return Part.NONE;
        }
        if (dy < 0) {
            return Part.FOUNDATION;
        }
        if (dy == 0) {
            return Part.FLOOR;
        }
        int ring = Math.max(ax, az);
        if (ring == RADIUS) {
            if (inGate(dx, dz, gateSide(seed))) {
                return Part.CLEAR;
            }
            return dy <= height(dx, dz, seed, 0, 5) ? Part.WALL : Part.CLEAR;
        }
        if (ax == PILLAR_OFFSET && az == PILLAR_OFFSET) {
            return dy <= 3 + height(dx, dz, seed, 1, 4) ? Part.PILLAR : Part.CLEAR;
        }
        if (dx == 0 && dz == 0) {
            if (dy == 1) {
                return Part.ALTAR;
            }
            if (dy == anchorHeight()) {
                return Part.ANCHOR;
            }
            return Part.CLEAR;
        }
        // The rubble keeps off the altar's surround and off the line of the gate, so the way in
        // and the way up to the anchor are never blocked.
        if (dy == 1 && ring > 1 && !inGate(dx, dz, gateSide(seed))
                && Long.remainderUnsigned(cellHash(dx, dz, seed, 2), 11) == 0) {
            return Part.RUBBLE;
        }
        return Part.CLEAR;
    }

    /** Height from 0 to {@code range - 1}, fixed per column and per use. */
    private static int height(int dx, int dz, long seed, int use, int range) {
        return (int) Long.remainderUnsigned(cellHash(dx, dz, seed, use), range);
    }

    /** True on the axis of the gate, from the wall to the altar. */
    private static boolean inGate(int dx, int dz, int side) {
        switch (side) {
            case 0:
                return dx > 0 && Math.abs(dz) <= GATE;
            case 1:
                return dx < 0 && Math.abs(dz) <= GATE;
            case 2:
                return dz > 0 && Math.abs(dx) <= GATE;
            default:
                return dz < 0 && Math.abs(dx) <= GATE;
        }
    }

    private static long cellHash(int dx, int dz, long seed, int use) {
        return RuinLattice.mix(seed ^ (dx * 0x1f1f_1f1fL) ^ (dz * 0x7a7a_7a7a_0000L) ^ ((long) use << 58));
    }
}
