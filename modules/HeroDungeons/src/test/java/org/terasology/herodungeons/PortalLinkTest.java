// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Matrix4f;
import org.joml.Vector3f;
import org.joml.Vector3i;
import org.joml.Vector4f;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether a ruin's window and its pocket's way out are one window seen from two places: the same cells moved by the
 * shift, a crossing that comes back exactly, and a landing that does not take one straight back.
 */
class PortalLinkTest {

    private static final int RX = 3364;
    private static final int RZ = 3963;
    private static final int FLOOR = 42;
    private static final Vector3i POCKET = new Vector3i(1600, 32, 1600);

    @Test
    void theRuinWindowMovedByTheShiftIsTheWayOut() {
        for (int side = 0; side < 4; side++) {
            PortalLink link = new PortalLink(RX, RZ, FLOOR, side, POCKET);
            for (Vector3i cell : link.ruinWindow().cells()) {
                Vector3i there = new Vector3i(cell).add(link.shift());
                assertTrue(link.pocketExit().contains(there.x, there.y, there.z), "side " + side + " cell " + cell);
                assertEquals(PocketRoom.Kind.WINDOW,
                        PocketRoom.at(there.x - POCKET.x, there.y - POCKET.y, there.z - POCKET.z, side),
                        "side " + side + " cell " + cell);
            }
        }
    }

    @Test
    void theWayOutStandsInTheRoomAndNowhereElse() {
        for (int side = 0; side < 4; side++) {
            int windows = 0;
            for (int dx = -PocketRoom.HALF; dx <= PocketRoom.HALF; dx++) {
                for (int dy = 0; dy < PocketRoom.WALL + PocketRoom.HEADROOM; dy++) {
                    for (int dz = -PocketRoom.HALF; dz <= PocketRoom.HALF; dz++) {
                        if (PocketRoom.at(dx, dy, dz, side) == PocketRoom.Kind.WINDOW) {
                            windows++;
                        }
                    }
                }
            }
            assertEquals(9, windows, "side " + side);
        }
    }

    @Test
    void goingThroughAndBackComesBackExactly() {
        for (int side = 0; side < 4; side++) {
            PortalLink link = new PortalLink(RX, RZ, FLOOR, side, POCKET);
            Vector3f axis = link.axis();
            // Just past the ruin's window on the altar side, walking towards the altar.
            Vector3f at = new Vector3f(RX, FLOOR + 1.6f, RZ).add(new Vector3f(axis).mul(RuinWindow.DISTANCE - 1.0f))
                    .add(axis.z * 0.4f, 0, axis.x * 0.4f);
            Vector3f inPocket = link.toPocket(at, -1);
            Vector3f back = link.toRuin(inPocket, -1);
            assertEquals(at.x, back.x, 1.0e-4f, "side " + side);
            assertEquals(at.y, back.y, 1.0e-4f, "side " + side);
            assertEquals(at.z, back.z, 1.0e-4f, "side " + side);
        }
    }

    @Test
    void oneLandsClearOfTheWindowOneCameThrough() {
        for (int side = 0; side < 4; side++) {
            PortalLink link = new PortalLink(RX, RZ, FLOOR, side, POCKET);
            for (int heading = -1; heading <= 1; heading += 2) {
                // In the middle of the ruin's slab, the worst place to cross from.
                Vector3f at = new Vector3f(RX, FLOOR + 1.6f, RZ).add(link.axis().mul(RuinWindow.DISTANCE));
                Vector3f there = link.toPocket(at, heading);
                assertTrue(Math.abs(link.alongPocket(there)) >= 0.5f + PortalLink.CLEARANCE - 1.0e-4f);
                assertEquals(heading, Math.signum(link.alongPocket(there)), 0.0f);
                // A step on, the same way, does not cross the way out again.
                Vector3f next = new Vector3f(there).add(link.axis().mul(0.2f * heading));
                assertFalse(link.pocketExit().crossedBy(there, next), "side " + side + " heading " + heading);

                Vector3f home = link.toRuin(there, heading);
                assertTrue(Math.abs(link.alongRuin(home)) >= 0.5f + PortalLink.CLEARANCE - 1.0e-4f);
                Vector3f onward = new Vector3f(home).add(link.axis().mul(0.2f * heading));
                assertFalse(link.ruinWindow().crossedBy(home, onward), "side " + side + " heading " + heading);
            }
        }
    }

    @Test
    void theTwoPanesAreOneMovedByTheShift() {
        for (int side = 0; side < 4; side++) {
            PortalLink link = new PortalLink(RX, RZ, FLOOR, side, POCKET);
            for (int face = -1; face <= 1; face += 2) {
                Vector3f[] ruin = link.ruinPane(face);
                Vector3f[] pocket = link.pocketPane(face);
                for (int corner = 0; corner < 4; corner++) {
                    Vector3f moved = new Vector3f(ruin[corner]).add(link.shift().x(), link.shift().y(),
                            link.shift().z());
                    assertEquals(0.0f, moved.distance(pocket[corner]), 1.0e-4f, "side " + side + " face " + face);
                }
                assertEquals(0.5f * face, link.alongRuin(ruin[0]), 1.0e-4f);
            }
        }
    }

    @Test
    void theObliqueNearPlaneLiesOnTheWindow() {
        Matrix4f projection = new Matrix4f().setPerspective((float) Math.toRadians(70), 16 / 9f, 0.1f, 1000f);
        Matrix4f view = new Matrix4f().lookAt(0, 0, 0, 1, 0, 0, 0, 1, 0);
        Vector3f eye = new Vector3f(10, 5, 3);
        // A plane four blocks ahead of the eye along +x, keeping what lies beyond it.
        Vector4f plane = new Vector4f(1, 0, 0, 14);
        PortalRenderSystem.oblique(projection, view, plane, eye);
        Vector4f onPlane = projection.transform(view.transform(new Vector4f(4, 0.5f, -0.3f, 1)));
        assertEquals(-1.0f, onPlane.z / onPlane.w, 1.0e-3f);
        Vector4f beyond = projection.transform(view.transform(new Vector4f(6, 0.5f, -0.3f, 1)));
        assertTrue(beyond.z / beyond.w > -1.0f);
        Vector4f before = projection.transform(view.transform(new Vector4f(2, 0.5f, -0.3f, 1)));
        assertTrue(before.z < -before.w, "a point between the eye and the window is clipped");
    }
}
