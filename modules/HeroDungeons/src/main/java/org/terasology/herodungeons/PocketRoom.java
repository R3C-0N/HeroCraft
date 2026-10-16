// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.engine.world.block.BlockRegion;

/**
 * The room laid down in a pocket for the bench: a walled courtyard floating in the dead slot.
 * <p>
 * Not a dungeon, a gauge. A flat floor shows whether the pocket is drawn flat; a grid of pillars,
 * one every {@link #PILLAR_STEP} blocks, shows whether distances and right angles survive; and the
 * open top lets the sun in, so nothing has to be lit to be seen. The dungeon proper is assembled
 * from drawn rooms later, by a generator; this class is what that generator will replace.
 * <p>
 * The box covers everything the room may write, air included, so laying the room down and wiping
 * it walk the very same blocks: a pocket wiped with a smaller box would keep whatever the players
 * built along its edges.
 */
public final class PocketRoom {

    /** Blocks from the centre to the inside face of a wall. */
    public static final int HALF = 16;

    /** Thickness of the floor slab, under the walking level. */
    public static final int FLOOR = 3;

    /** Height of the walls above the walking level. */
    public static final int WALL = 5;

    /** Headroom kept clear, and wiped, above the walls. */
    public static final int HEADROOM = 8;

    /** Spacing of the pillars. */
    public static final int PILLAR_STEP = 8;

    /** Height of a pillar. */
    public static final int PILLAR = 3;

    /** From the centre to the way out, behind where one arrives. */
    public static final int EXIT_DISTANCE = 4;

    /**
     * The side the way out stands on in a bench pocket, numbered as {@link RuinWindow} numbers a gate: north. A
     * ruin's pocket puts it on the side of the ruin's gate, so that the two windows face the same way.
     */
    public static final int BENCH_EXIT_SIDE = 3;

    /** Half the width of the way out, and its height above the walking level. */
    public static final int EXIT_HALF_WIDTH = 1;
    public static final int EXIT_HEIGHT = 3;

    public enum Kind { AIR, FLOOR, WALL, PILLAR, WINDOW }

    private PocketRoom() {
    }

    /** Every block the room may write, around a centre whose Y is the walking level. */
    public static BlockRegion box(Vector3ic centre) {
        return new BlockRegion(
                centre.x() - HALF - 1, centre.y() - FLOOR, centre.z() - HALF - 1,
                centre.x() + HALF + 1, centre.y() + WALL + HEADROOM, centre.z() + HALF + 1);
    }

    /** What stands at a position, relative to the centre of the room, in a bench pocket. */
    public static Kind at(int dx, int dy, int dz) {
        return at(dx, dy, dz, BENCH_EXIT_SIDE);
    }

    /** What stands at a position, relative to the centre of the room, the way out on {@code exitSide}. */
    public static Kind at(int dx, int dy, int dz, int exitSide) {
        int ax = Math.abs(dx);
        int az = Math.abs(dz);
        if (ax > HALF + 1 || az > HALF + 1 || dy < -FLOOR) {
            return Kind.AIR;
        }
        if (dy < 0) {
            return Kind.FLOOR;
        }
        if (ax == HALF + 1 || az == HALF + 1) {
            return dy < WALL ? Kind.WALL : Kind.AIR;
        }
        boolean onGrid = dx % PILLAR_STEP == 0 && dz % PILLAR_STEP == 0 && (dx != 0 || dz != 0);
        if (onGrid && dy < PILLAR) {
            return Kind.PILLAR;
        }
        if (isExit(dx, dy, dz, exitSide)) {
            return Kind.WINDOW;
        }
        return Kind.AIR;
    }

    /**
     * Whether a position, relative to the centre, is in the window that leads back out of a bench pocket: a step
     * behind where one arrives, so that one turns round to leave, and never walks into it on arriving.
     */
    public static boolean isExit(int dx, int dy, int dz) {
        return isExit(dx, dy, dz, BENCH_EXIT_SIDE);
    }

    /** Whether a position, relative to the centre, is in the way out standing on {@code exitSide}. */
    public static boolean isExit(int dx, int dy, int dz, int exitSide) {
        return exit(new Vector3i(), exitSide).contains(dx, dy, dz);
    }

    /** The way out of a bench pocket as a walker meets it, for a room centred on {@code centre}. */
    static WindowPlane exit(Vector3ic centre) {
        return exit(centre, BENCH_EXIT_SIDE);
    }

    /** The way out standing on {@code exitSide}, as a walker meets it. */
    static WindowPlane exit(Vector3ic centre, int exitSide) {
        return new WindowPlane(centre.x(), centre.z(), RuinWindow.axisX(exitSide), RuinWindow.axisZ(exitSide),
                EXIT_DISTANCE, EXIT_HALF_WIDTH, centre.y(), EXIT_HEIGHT);
    }

    /**
     * Where a character is set down: over the centre of the floor, clear of the pillars, one block
     * up because a character's position is the middle of its body, not its feet.
     */
    public static Vector3i arrival(Vector3ic centre) {
        return new Vector3i(centre.x(), centre.y() + 1, centre.z());
    }
}
