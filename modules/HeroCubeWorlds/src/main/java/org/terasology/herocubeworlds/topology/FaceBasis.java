// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.topology;

import org.terasology.context.annotation.API;

/**
 * The integer frame of a cube face: {@code right}, {@code up} and {@code normal}, with
 * {@code right x up = normal}.
 * <p>
 * These vectors live in <em>planet space</em>, which is not world space: its +Z is the polar axis,
 * so {@code direction.z} is directly {@code sin(latitude)}. World space keeps the net in XZ with Y
 * as depth. Keeping the two apart is deliberate — planet space is the only place where a direction
 * means anything, and world space is the only place where a block does.
 * <p>
 * A heading is never stored as an angle in face coordinates. Near a corner the two tangents of a
 * face meet at 120 degrees, so a quarter-turn correction applied to coordinates is wrong by up to
 * 25.6 degrees. Orientation belongs to the world; the grid only ever carries positions.
 */
@API
public record FaceBasis(int rx, int ry, int rz, int hx, int hy, int hz, int nx, int ny, int nz) {

    /**
     * The canonical frame of each face, indexed by face id. On the four equatorial faces {@code up}
     * points north.
     */
    public static final FaceBasis[] BASES = {
            // +X
            new FaceBasis(0, 1, 0, 0, 0, 1, 1, 0, 0),
            // +Y
            new FaceBasis(-1, 0, 0, 0, 0, 1, 0, 1, 0),
            // -X
            new FaceBasis(0, -1, 0, 0, 0, 1, -1, 0, 0),
            // -Y
            new FaceBasis(1, 0, 0, 0, 0, 1, 0, -1, 0),
            // +Z, north cap
            new FaceBasis(-1, 0, 0, 0, -1, 0, 0, 0, 1),
            // -Z, south cap
            new FaceBasis(-1, 0, 0, 0, 1, 0, 0, 0, -1)
    };

    /** cos and sin of each quarter turn, as exact integers. */
    public static final int[][] COS_SIN = {{1, 0}, {0, 1}, {-1, 0}, {0, -1}};

    /**
     * Crossing the {@code +right} edge. The axis of the edge is kept; the other two pivot a quarter
     * turn about it. All four flips together cover the twenty-four edge crossings of the cube.
     */
    public FaceBasis acrossRightPlus() {
        return new FaceBasis(-nx, -ny, -nz, hx, hy, hz, rx, ry, rz);
    }

    /** Crossing the {@code -right} edge. */
    public FaceBasis acrossRightMinus() {
        return new FaceBasis(nx, ny, nz, hx, hy, hz, -rx, -ry, -rz);
    }

    /** Crossing the {@code +up} edge. */
    public FaceBasis acrossUpPlus() {
        return new FaceBasis(rx, ry, rz, -nx, -ny, -nz, hx, hy, hz);
    }

    /** Crossing the {@code -up} edge. */
    public FaceBasis acrossUpMinus() {
        return new FaceBasis(rx, ry, rz, nx, ny, nz, -hx, -hy, -hz);
    }

    /** @return the face whose canonical normal equals this basis' normal */
    public static int faceOfNormal(int nx, int ny, int nz) {
        for (int face = 0; face < CubePattern.FACES; face++) {
            FaceBasis b = BASES[face];
            if (b.nx == nx && b.ny == ny && b.nz == nz) {
                return face;
            }
        }
        throw new IllegalArgumentException("not a face normal: " + nx + ", " + ny + ", " + nz);
    }

    /** @return the face this basis sits on */
    public int face() {
        return faceOfNormal(nx, ny, nz);
    }

    /**
     * How many quarter turns separate this basis from its face's canonical one.
     * <p>
     * Read off the basis, never accumulated along the way: a walk that leaves and re-enters a face
     * would otherwise carry a stale count.
     */
    public int quarterTurns() {
        FaceBasis canonical = BASES[face()];
        int cx = canonical.rx;
        int cy = canonical.ry;
        int cz = canonical.rz;
        for (int k = 0; k < 4; k++) {
            if (cx == rx && cy == ry && cz == rz) {
                return k;
            }
            // c <- n x c
            int px = ny * cz - nz * cy;
            int py = nz * cx - nx * cz;
            int pz = nx * cy - ny * cx;
            cx = px;
            cy = py;
            cz = pz;
        }
        throw new IllegalStateException("basis is not a quarter turn of its face");
    }

    /**
     * Rotates local coordinates by {@code k} quarter turns about the centre of a face of side
     * {@code size}.
     * <p>
     * Coordinates are centred <em>and doubled</em> so that everything stays an exact integer — a
     * face of even side has no centre cell, and rounding here would drift a cell per crossing.
     */
    public static void rotate(int u, int v, int k, int size, int[] dest) {
        int a = 2 * u - (size - 1);
        int b = 2 * v - (size - 1);
        int cos = COS_SIN[k][0];
        int sin = COS_SIN[k][1];
        int ra = a * cos - b * sin;
        int rb = a * sin + b * cos;
        dest[0] = (ra + size - 1) / 2;
        dest[1] = (rb + size - 1) / 2;
    }
}
