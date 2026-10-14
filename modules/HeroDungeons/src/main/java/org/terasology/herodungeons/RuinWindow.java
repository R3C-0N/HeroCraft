// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.joml.Vector3i;
import org.terasology.herocubeworlds.generator.RuinShape;

import java.util.ArrayList;
import java.util.List;

/**
 * Where the window onto the past stands in a ruin, and where one comes out of it.
 * <p>
 * Across the axis of the gate, {@link #DISTANCE} blocks from the altar: the one line of the court that
 * {@link RuinShape} keeps clear from the wall to the anchor stone, so the window never lands on a pillar,
 * on rubble or in a wall, and one walks into it straight from the way in. Three blocks wide, three high.
 * <p>
 * One comes back out on the gate side, {@link #OUTSIDE} blocks from the altar, facing the window: a step
 * forward and one is through it again, which is the two-way crossing D29 asks for.
 */
public final class RuinWindow {

    /** From the altar to the plane of the window, along the axis of the gate. */
    public static final int DISTANCE = 3;

    /** From the altar to where one comes back out, along the same axis. */
    public static final int OUTSIDE = 5;

    /** Half the width of the window. */
    public static final int HALF_WIDTH = 1;

    /** Height of the window above the walking level. */
    public static final int HEIGHT = 3;

    /** Shortest and longest a window stays open, in minutes (D16). */
    public static final int MIN_MINUTES = 60;
    public static final int MAX_MINUTES = 120;

    private final int centreX;
    private final int centreZ;
    private final int floorY;
    private final int axisX;
    private final int axisZ;
    private final WindowPlane plane;

    /**
     * @param floorY the floor of the ruin; one walks on the level above it
     * @param gateSide {@link RuinShape#gateSide}: 0 east, 1 west, 2 south, 3 north
     */
    public RuinWindow(int centreX, int centreZ, int floorY, int gateSide) {
        this.centreX = centreX;
        this.centreZ = centreZ;
        this.floorY = floorY;
        this.axisX = gateSide == 0 ? 1 : gateSide == 1 ? -1 : 0;
        this.axisZ = gateSide == 2 ? 1 : gateSide == 3 ? -1 : 0;
        this.plane = new WindowPlane(centreX, centreZ, axisX, axisZ, DISTANCE, HALF_WIDTH, floorY + 1, HEIGHT);
    }

    /** Every block of the window. */
    public List<Vector3i> cells() {
        List<Vector3i> cells = new ArrayList<>();
        for (int t = -HALF_WIDTH; t <= HALF_WIDTH; t++) {
            for (int dy = 1; dy <= HEIGHT; dy++) {
                cells.add(new Vector3i(centreX + axisX * DISTANCE + axisZ * t, floorY + dy,
                        centreZ + axisZ * DISTANCE + axisX * t));
            }
        }
        return cells;
    }

    /** Whether a block position is part of the window. */
    public boolean contains(int x, int y, int z) {
        return plane.contains(x, y, z);
    }

    /** Whether a body moving between two positions walks through the window, or stands in it. */
    public boolean crossedBy(Vector3fc from, Vector3fc to) {
        return plane.crossedBy(from, to);
    }

    /**
     * Where a character is set down on coming back: on the gate side, one block up from the walking
     * level because a character's position is the middle of its body.
     */
    public Vector3f outside() {
        return new Vector3f(centreX + axisX * OUTSIDE, floorY + 2, centreZ + axisZ * OUTSIDE);
    }

    /**
     * How long a ruin's window stays open, fixed by the ruin rather than drawn at each opening: a ruin is
     * a short dungeon or a long one, and stays so (D16 gives one to two hours "according to the dungeon").
     */
    public static int minutes(long ruinSeed) {
        return MIN_MINUTES + (int) Math.floorMod(ruinSeed >>> 8, (long) (MAX_MINUTES - MIN_MINUTES + 1));
    }

    /** The anchor stone, where the window entity keeps the ruin loaded. */
    public Vector3f anchor() {
        return new Vector3f(centreX, floorY + RuinShape.anchorHeight(), centreZ);
    }
}
