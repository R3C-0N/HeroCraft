// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector2i;
import org.joml.Vector2ic;
import org.terasology.herocubeworlds.topology.CubePattern;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Where the pockets of the past may be laid down: a plot at the centre of each dead slot of the cube
 * net.
 * <p>
 * <b>Why the dead slots.</b> The engine loads one world and only one. Ten of the sixteen slots of
 * the net carry no face and generate air, each as large as a face, so a pocket put there is a place
 * of its own without a second world: the network, the entities, the saves and the bestiary see an
 * ordinary part of the map. The renderer draws a dead slot flat, and only from inside one.
 * <p>
 * <b>Why one plot per slot.</b> Two things must never be loaded together, a pocket and
 * the planet, or two pockets: the chunks around a player reach half the loaded box, {@link #REACH}
 * blocks at the widest view distance. A player stands anywhere in a room up to
 * {@link #MAX_HALF_WIDTH} from its centre, so the edge of the slot must lie further than the margin
 * less that half width, and the next room further than a slot less both half widths. Two plots a
 * slot would not hold at the widest view distance; ten pockets open at once is plenty for a server
 * among friends (D22).
 * {@code PocketPlotsTest} checks both against the reach.
 */
public final class PocketPlots {

    /**
     * How far a player's chunks reach, in blocks: half the loaded box at the widest view distance,
     * {@code EXTREME}, sixty-three chunks across — 1008 blocks, rounded up to a whole kilometre.
     */
    public static final int REACH = 1024;

    /** The widest a pocket may reach from its centre, in blocks, so that the spacing above holds. */
    public static final int MAX_HALF_WIDTH = 512;

    private final List<Vector2ic> centres;
    private final int faceBlocks;

    public PocketPlots(int faceBlocks) {
        this.faceBlocks = faceBlocks;
        List<Vector2ic> found = new ArrayList<>();
        for (int row = 0; row < CubePattern.PATTERN_SIDE; row++) {
            for (int column = 0; column < CubePattern.PATTERN_SIDE; column++) {
                if (CubePattern.faceAt(column, row) == CubePattern.NO_FACE) {
                    found.add(new Vector2i(column * faceBlocks + faceBlocks / 2, row * faceBlocks + faceBlocks / 2));
                }
            }
        }
        this.centres = Collections.unmodifiableList(found);
    }

    /** How many pockets can be open at once. */
    public int count() {
        return centres.size();
    }

    /** From the centre of a plot to the edge of its slot, in blocks. */
    public int margin() {
        return faceBlocks / 2;
    }

    /** The centre of a plot, as world X and world Z. */
    public Vector2ic centre(int plot) {
        return centres.get(plot);
    }

    /** @return the plot whose square holds the given column, or -1 for none */
    public int plotAt(int worldX, int worldZ) {
        for (int plot = 0; plot < centres.size(); plot++) {
            Vector2ic c = centres.get(plot);
            if (Math.abs(worldX - c.x()) <= MAX_HALF_WIDTH && Math.abs(worldZ - c.y()) <= MAX_HALF_WIDTH) {
                return plot;
            }
        }
        return -1;
    }
}
