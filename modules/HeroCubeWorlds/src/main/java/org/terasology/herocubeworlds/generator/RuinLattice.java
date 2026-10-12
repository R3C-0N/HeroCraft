// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator;

import org.terasology.herocubeworlds.topology.CubePattern;

import java.util.ArrayList;
import java.util.List;

/**
 * Where a ruin may stand: one candidate site per cell of a grid laid over the net.
 * <p>
 * <b>A grid, not a noise field.</b> A ruin is a place one goes back to, so its position has to be a
 * fact of the seed that any chunk, any command and any later lot can recompute without generating a
 * block. A hashed point in a fixed cell is exactly that, and it also spaces the ruins out: two sites
 * are never closer than twice the {@link #MARGIN}, and never further than two cells.
 * <p>
 * <b>Inside a face, always.</b> The cells divide a face edge, and a site keeps {@link #MARGIN} blocks
 * from the edges of its cell, more than the reach of a ruin; so a ruin never straddles a seam, where
 * the ground on the other side belongs to another face and is turned a quarter. A site whose
 * footprint would leave its face anyway, on a geometry whose face edge the cell does not divide, is
 * dropped rather than bent.
 * <p>
 * The site is only a candidate. Whether a ruin stands there is decided by the ground —
 * {@code CubeRuinProvider} rejects the sea, the slopes and the biomes no people settled — which is
 * what thins one site per cell down to the handful a face actually carries.
 */
public final class RuinLattice {

    /** Edge of a cell, in blocks: five to a face edge of 3200. */
    public static final int CELL = 640;

    /** Blocks kept between a site and the edges of its cell. */
    public static final int MARGIN = 48;

    private static final long SALT = 0x5e1f_7a3c_9b24_d681L;

    private final long seed;
    private final int faceBlocks;

    public RuinLattice(long worldSeed, int faceBlocks) {
        this.seed = worldSeed ^ SALT;
        this.faceBlocks = faceBlocks;
    }

    /** A candidate site: its centre column, the face it stands on, and a seed for its shape. */
    public static final class Site {
        private final int x;
        private final int z;
        private final int face;
        private final long seed;

        Site(int x, int z, int face, long seed) {
            this.x = x;
            this.z = z;
            this.face = face;
            this.seed = seed;
        }

        public int x() {
            return x;
        }

        public int z() {
            return z;
        }

        public int face() {
            return face;
        }

        public long seed() {
            return seed;
        }
    }

    /** @return the candidate of a cell, or null when the cell lies on a dead slot of the net */
    public Site site(int cellX, int cellZ) {
        long hash = mix(seed + cellX * 0x9e37_79b9_7f4a_7c15L + cellZ * 0xc2b2_ae3d_27d4_eb4fL);
        int span = CELL - 2 * MARGIN;
        int x = cellX * CELL + MARGIN + (int) Long.remainderUnsigned(hash, span);
        int z = cellZ * CELL + MARGIN + (int) Long.remainderUnsigned(hash >>> 32, span);
        int face = CubePattern.faceOf(x, z, faceBlocks);
        if (face == CubePattern.NO_FACE) {
            return null;
        }
        int reach = RuinShape.RADIUS;
        if (CubePattern.faceOf(x - reach, z - reach, faceBlocks) != face
                || CubePattern.faceOf(x + reach, z + reach, faceBlocks) != face) {
            return null;
        }
        return new Site(x, z, face, mix(hash));
    }

    /** Every candidate whose centre lies within {@code reach} blocks of the rectangle, edges included. */
    public List<Site> sitesNear(int minX, int minZ, int maxX, int maxZ, int reach) {
        List<Site> found = new ArrayList<>();
        int cellX0 = Math.floorDiv(minX - reach, CELL);
        int cellX1 = Math.floorDiv(maxX + reach, CELL);
        int cellZ0 = Math.floorDiv(minZ - reach, CELL);
        int cellZ1 = Math.floorDiv(maxZ + reach, CELL);
        for (int cellZ = cellZ0; cellZ <= cellZ1; cellZ++) {
            for (int cellX = cellX0; cellX <= cellX1; cellX++) {
                Site site = site(cellX, cellZ);
                if (site != null
                        && site.x >= minX - reach && site.x <= maxX + reach
                        && site.z >= minZ - reach && site.z <= maxZ + reach) {
                    found.add(site);
                }
            }
        }
        return found;
    }

    /** SplitMix64's finaliser: every bit of the input moves every bit of the output. */
    static long mix(long value) {
        long z = value + 0x9e37_79b9_7f4a_7c15L;
        z = (z ^ (z >>> 30)) * 0xbf58_476d_1ce4_e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d0_49bb_1331_11ebL;
        return z ^ (z >>> 31);
    }
}
