// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector2ic;
import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.engine.rendering.world.viewDistance.ViewDistance;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether a pocket can be loaded without loading anything else.
 * <p>
 * A player in a pocket loads half the box of the view distance around them, and so does a player on
 * the planet. Neither may reach into a room that is not theirs: the planet seen from a pocket is
 * exiled by the shader, but a second pocket is flat too, and would be drawn.
 */
class PocketPlotsTest {

    private static final int FACE = CubeGeometry.DEFAULT_FACE_CHUNKS * Chunks.SIZE_X;
    private final PocketPlots plots = new PocketPlots(FACE);

    @Test
    void theReachCoversTheWidestViewDistance() {
        int chunks = 0;
        for (ViewDistance distance : ViewDistance.values()) {
            chunks = Math.max(chunks, distance.getChunkDistance().x());
        }
        double reach = (chunks / 2.0 + 0.5) * Chunks.SIZE_X;
        assertTrue(PocketPlots.REACH >= reach, "the widest view distance reaches " + reach + " blocks");
    }

    @Test
    void oneOnEachDeadSlot() {
        assertEquals(CubePattern.PATTERN_SIDE * CubePattern.PATTERN_SIDE - CubePattern.FACES, plots.count());
        for (int plot = 0; plot < plots.count(); plot++) {
            Vector2ic c = plots.centre(plot);
            assertEquals(CubePattern.NO_FACE, CubePattern.faceOf(c.x(), c.y(), FACE), "plot " + plot);
        }
    }

    @Test
    void nothingLiveWithinReachOfARoom() {
        // A player anywhere in the room, and a player on any face: neither box reaches the other side.
        assertTrue(plots.margin() - PocketPlots.MAX_HALF_WIDTH > PocketPlots.REACH);
        for (int plot = 0; plot < plots.count(); plot++) {
            Vector2ic c = plots.centre(plot);
            int far = PocketPlots.MAX_HALF_WIDTH + PocketPlots.REACH;
            for (int dx = -far; dx <= far; dx += 16) {
                for (int dz = -far; dz <= far; dz += 16) {
                    assertEquals(CubePattern.NO_FACE, CubePattern.faceOf(c.x() + dx, c.y() + dz, FACE),
                            "plot " + plot + " reaches a face at " + dx + ", " + dz);
                }
            }
        }
    }

    @Test
    void noRoomWithinReachOfAnother() {
        for (int a = 0; a < plots.count(); a++) {
            for (int b = a + 1; b < plots.count(); b++) {
                Vector2ic ca = plots.centre(a);
                Vector2ic cb = plots.centre(b);
                int gap = Math.max(Math.abs(ca.x() - cb.x()), Math.abs(ca.y() - cb.y()));
                assertTrue(gap - 2 * PocketPlots.MAX_HALF_WIDTH > PocketPlots.REACH, a + " and " + b);
            }
        }
    }

    @Test
    void aPlotIsFoundFromInsideIt() {
        for (int plot = 0; plot < plots.count(); plot++) {
            Vector2ic c = plots.centre(plot);
            assertEquals(plot, plots.plotAt(c.x() + PocketPlots.MAX_HALF_WIDTH, c.y() - PocketPlots.MAX_HALF_WIDTH));
        }
        assertEquals(-1, plots.plotAt(FACE + FACE / 2, FACE + FACE / 2));
    }

    @Test
    void theRoomFitsItsPlotAndItsBox() {
        BlockRegion box = PocketRoom.box(new Vector3i());
        assertTrue(box.maxX() <= PocketPlots.MAX_HALF_WIDTH && box.maxZ() <= PocketPlots.MAX_HALF_WIDTH);
        // Nothing the room builds lies outside the box it is wiped with.
        for (int dx = -40; dx <= 40; dx++) {
            for (int dy = -10; dy <= 30; dy++) {
                for (int dz = -40; dz <= 40; dz++) {
                    if (PocketRoom.at(dx, dy, dz) != PocketRoom.Kind.AIR) {
                        assertTrue(box.contains(dx, dy, dz), dx + ", " + dy + ", " + dz);
                    }
                }
            }
        }
        assertEquals(PocketRoom.Kind.AIR, PocketRoom.at(0, 0, 0), "the arrival is clear");
        assertEquals(PocketRoom.Kind.FLOOR, PocketRoom.at(0, -1, 0), "and stands on the floor");
    }
}
