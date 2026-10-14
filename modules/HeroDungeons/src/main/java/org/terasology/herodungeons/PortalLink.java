// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.joml.Vector3i;
import org.joml.Vector3ic;

/**
 * How a ruin's window and its pocket's way out answer each other.
 * <p>
 * The pocket puts its way out on the side of the ruin's gate, {@link PocketRoom#EXIT_DISTANCE} blocks from its
 * centre, so that the two windows face the same way and one is the other moved by a whole number of blocks:
 * {@link #shift()}. Crossing one sets the character down at the same place relative to the other, moving the same
 * way; and what one sees through either window is the other side seen from the eye moved by the same shift. No
 * turn, no scale: a translation is the whole of the link, and it is exactly undone on the way back (D29).
 * <p>
 * The pocket is flat and the planet is bent by the shader. Near the eye the bent world keeps the grid's axes and
 * only scales them, which an eye cannot tell, so the translation stays true for a window seen from close; from
 * forty blocks it is a turn of under a degree.
 */
public final class PortalLink {

    /** How far past a window's slab a character is set down, so that the window it lands by does not take it back. */
    public static final float CLEARANCE = 0.3f;

    private final int ruinX;
    private final int ruinZ;
    private final int floorY;
    private final int gateSide;
    private final int axisX;
    private final int axisZ;
    private final Vector3i pocketCentre;
    private final Vector3i shift;

    /**
     * @param floorY the floor of the ruin; one walks on the level above it
     * @param pocketCentre the centre of the pocket's room, at its walking level
     */
    public PortalLink(int ruinX, int ruinZ, int floorY, int gateSide, Vector3ic pocketCentre) {
        this.ruinX = ruinX;
        this.ruinZ = ruinZ;
        this.floorY = floorY;
        this.gateSide = gateSide;
        this.axisX = RuinWindow.axisX(gateSide);
        this.axisZ = RuinWindow.axisZ(gateSide);
        this.pocketCentre = new Vector3i(pocketCentre);
        this.shift = new Vector3i(
                pocketCentre.x() + axisX * PocketRoom.EXIT_DISTANCE - (ruinX + axisX * RuinWindow.DISTANCE),
                pocketCentre.y() - (floorY + 1),
                pocketCentre.z() + axisZ * PocketRoom.EXIT_DISTANCE - (ruinZ + axisZ * RuinWindow.DISTANCE));
    }

    /** What moves the ruin's side onto the pocket's. */
    public Vector3ic shift() {
        return shift;
    }

    public Vector3ic pocketCentre() {
        return pocketCentre;
    }

    public int gateSide() {
        return gateSide;
    }

    public RuinWindow ruinWindow() {
        return new RuinWindow(ruinX, ruinZ, floorY, gateSide);
    }

    /** The pocket's way out, as a walker meets it. */
    public WindowPlane pocketExit() {
        return PocketRoom.exit(pocketCentre, gateSide);
    }

    /** Which way a step goes along the axis of the gate: 1 towards the gate, -1 towards the altar, or the fallback. */
    public int heading(Vector3fc from, Vector3fc to, int fallback) {
        float along = (to.x() - from.x()) * axisX + (to.z() - from.z()) * axisZ;
        return along > 1.0e-4f ? 1 : along < -1.0e-4f ? -1 : fallback;
    }

    /** Where a character crossing the ruin's window comes out in the pocket. */
    public Vector3f toPocket(Vector3fc at, int heading) {
        Vector3f there = new Vector3f(at).add(shift.x, shift.y, shift.z);
        return clear(there, pocketCentre.x, pocketCentre.z, PocketRoom.EXIT_DISTANCE, heading);
    }

    /** Where a character crossing the pocket's way out comes out at the ruin. */
    public Vector3f toRuin(Vector3fc at, int heading) {
        Vector3f there = new Vector3f(at).sub(shift.x, shift.y, shift.z);
        return clear(there, ruinX, ruinZ, RuinWindow.DISTANCE, heading);
    }

    /** Distance along the gate's axis from the ruin's window, positive on the gate side. */
    public float alongRuin(Vector3fc at) {
        return (at.x() - ruinX) * axisX + (at.z() - ruinZ) * axisZ - RuinWindow.DISTANCE;
    }

    /** Distance along the gate's axis from the pocket's way out, positive on the far side from the room's centre. */
    public float alongPocket(Vector3fc at) {
        return (at.x() - pocketCentre.x) * axisX + (at.z() - pocketCentre.z) * axisZ - PocketRoom.EXIT_DISTANCE;
    }

    /**
     * The face of the ruin's window on one side, as four corners: bottom left, bottom right, top right, top left.
     *
     * @param side 1 for the face towards the gate, -1 for the face towards the altar
     */
    public Vector3f[] ruinPane(int side) {
        return pane(ruinX, floorY + 1, ruinZ, RuinWindow.DISTANCE, side);
    }

    /**
     * The face of the pocket's way out on one side, in the same order.
     *
     * @param side 1 for the face away from the room's centre, -1 for the face towards it
     */
    public Vector3f[] pocketPane(int side) {
        return pane(pocketCentre.x, pocketCentre.y, pocketCentre.z, PocketRoom.EXIT_DISTANCE, side);
    }

    /** The unit step along the gate's axis, towards the gate. */
    public Vector3f axis() {
        return new Vector3f(axisX, 0, axisZ);
    }

    private Vector3f[] pane(int originX, int bottom, int originZ, int distance, int side) {
        float along = distance + 0.5f * side;
        float half = RuinWindow.HALF_WIDTH + 0.5f;
        float low = bottom - 0.5f;
        float high = bottom - 0.5f + RuinWindow.HEIGHT;
        // Across runs along Z for an east or west gate and along X for a south or north one, as WindowPlane has it.
        float centreX = originX + axisX * along;
        float centreZ = originZ + axisZ * along;
        return new Vector3f[] {
            new Vector3f(centreX - axisZ * half, low, centreZ - axisX * half),
            new Vector3f(centreX + axisZ * half, low, centreZ + axisX * half),
            new Vector3f(centreX + axisZ * half, high, centreZ + axisX * half),
            new Vector3f(centreX - axisZ * half, high, centreZ - axisX * half)
        };
    }

    /** Pushes a position past a window's slab, in the direction of travel, unless it is already past. */
    private Vector3f clear(Vector3f there, int originX, int originZ, int distance, int heading) {
        float along = (there.x - originX) * axisX + (there.z - originZ) * axisZ;
        float target = heading < 0
                ? Math.min(along, distance - 0.5f - CLEARANCE)
                : Math.max(along, distance + 0.5f + CLEARANCE);
        there.x += axisX * (target - along);
        there.z += axisZ * (target - along);
        return there;
    }
}
