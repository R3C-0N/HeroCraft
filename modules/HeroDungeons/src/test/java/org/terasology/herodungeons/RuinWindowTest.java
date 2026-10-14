// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3f;
import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.RuinShape;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the window onto the past can always be opened and always be walked through: it must land
 * in air whatever the ruin, and one must come back out in air, in front of it.
 */
class RuinWindowTest {

    private static final int CX = 100;
    private static final int CZ = -40;
    private static final int FLOOR = 42;
    private static final int SEEDS = 4096;

    @Test
    void theWindowStandsInTheClearForEveryRuin() {
        for (long seed = 0; seed < SEEDS; seed++) {
            RuinWindow window = new RuinWindow(CX, CZ, FLOOR, RuinShape.gateSide(seed));
            assertEquals(9, window.cells().size());
            for (Vector3i cell : window.cells()) {
                assertEquals(RuinShape.Part.CLEAR, RuinShape.part(cell.x - CX, cell.y - FLOOR, cell.z - CZ, seed),
                        "seed " + seed + " cell " + cell);
                assertTrue(window.contains(cell.x, cell.y, cell.z));
            }
        }
    }

    @Test
    void oneComesBackOutInTheClearOnTheGateSide() {
        for (long seed = 0; seed < SEEDS; seed++) {
            int side = RuinShape.gateSide(seed);
            RuinWindow window = new RuinWindow(CX, CZ, FLOOR, side);
            Vector3f out = window.outside();
            int x = Math.round(out.x);
            int z = Math.round(out.z);
            // The feet and the head, a block below and at the middle of the body.
            for (int y = Math.round(out.y) - 1; y <= Math.round(out.y); y++) {
                assertEquals(RuinShape.Part.CLEAR, RuinShape.part(x - CX, y - FLOOR, z - CZ, seed), "seed " + seed);
                assertFalse(window.contains(x, y, z), "one comes back out inside the window");
            }
            assertEquals(RuinShape.Part.FLOOR, RuinShape.part(x - CX, 0, z - CZ, seed));
        }
    }

    @Test
    void theWindowFacesTheGate() {
        assertTrue(new RuinWindow(0, 0, 0, 0).contains(RuinWindow.DISTANCE, 1, 0));
        assertTrue(new RuinWindow(0, 0, 0, 1).contains(-RuinWindow.DISTANCE, 1, 0));
        assertTrue(new RuinWindow(0, 0, 0, 2).contains(0, 1, RuinWindow.DISTANCE));
        assertTrue(new RuinWindow(0, 0, 0, 3).contains(0, 1, -RuinWindow.DISTANCE));
        assertFalse(new RuinWindow(0, 0, 0, 0).contains(-RuinWindow.DISTANCE, 1, 0));
        assertFalse(new RuinWindow(0, 0, 0, 0).contains(RuinWindow.DISTANCE, 0, 0));
        assertFalse(new RuinWindow(0, 0, 0, 0).contains(RuinWindow.DISTANCE, 1 + RuinWindow.HEIGHT, 0));
    }

    @Test
    void aWindowStaysOpenOneToTwoHours() {
        for (long seed = -SEEDS; seed < SEEDS; seed++) {
            int minutes = RuinWindow.minutes(seed * 0x9E3779B97F4A7C15L);
            assertTrue(minutes >= RuinWindow.MIN_MINUTES && minutes <= RuinWindow.MAX_MINUTES, "" + minutes);
        }
    }

    @Test
    void theWayOutOfAPocketIsClearOfPillarsAndOfTheArrival() {
        for (int dx = -RuinWindow.HALF_WIDTH; dx <= RuinWindow.HALF_WIDTH; dx++) {
            for (int dy = 0; dy < PocketRoom.EXIT_HEIGHT; dy++) {
                assertEquals(PocketRoom.Kind.WINDOW, PocketRoom.at(dx, dy, -PocketRoom.EXIT_DISTANCE));
            }
        }
        Vector3i arrival = PocketRoom.arrival(new Vector3i());
        for (int dy = arrival.y - 1; dy <= arrival.y; dy++) {
            assertEquals(PocketRoom.Kind.AIR, PocketRoom.at(arrival.x, dy, arrival.z));
        }
    }

    @Test
    void aLongStepThroughTheWindowStillCrossesIt() {
        // East gate: the window stands at x = 3, across z in [-1, 1]; one walks in from the gate, westward.
        RuinWindow window = new RuinWindow(0, 0, 0, 0);
        assertTrue(window.crossedBy(new Vector3f(6, 1.4f, 0), new Vector3f(1, 1.4f, 0)), "five blocks a tick");
        assertTrue(window.crossedBy(new Vector3f(3.9f, 1.4f, 1), new Vector3f(3.2f, 1.4f, 1)), "a short step in");
        assertTrue(window.crossedBy(new Vector3f(3, 1.4f, 0), new Vector3f(3, 1.4f, 0)), "standing in it");
    }

    @Test
    void aStepBesideOrShortOfTheWindowDoesNot() {
        RuinWindow window = new RuinWindow(0, 0, 0, 0);
        assertFalse(window.crossedBy(new Vector3f(6, 1.4f, 3), new Vector3f(1, 1.4f, 3)), "beside it");
        assertFalse(window.crossedBy(new Vector3f(8, 1.4f, 0), new Vector3f(4, 1.4f, 0)), "short of it");
        assertFalse(window.crossedBy(new Vector3f(6, 6.4f, 0), new Vector3f(1, 6.4f, 0)), "over it");
        Vector3f out = window.outside();
        assertFalse(window.crossedBy(out, out), "where one comes back out");
    }

    @Test
    void aLongStepThroughTheWayOutOfAPocketCrossesIt() {
        Vector3i centre = new Vector3i(1600, 32, 1600);
        WindowPlane exit = PocketRoom.exit(centre);
        Vector3i arrival = PocketRoom.arrival(centre);
        Vector3f from = new Vector3f(arrival.x, arrival.y + 0.4f, arrival.z);
        assertFalse(exit.crossedBy(from, from), "one arrives clear of it");
        assertTrue(exit.crossedBy(from, new Vector3f(from).add(0, 0, -6)), "turning round and walking out");
        assertFalse(exit.crossedBy(from, new Vector3f(from).add(0, 0, 6)), "walking the other way");
    }
}
