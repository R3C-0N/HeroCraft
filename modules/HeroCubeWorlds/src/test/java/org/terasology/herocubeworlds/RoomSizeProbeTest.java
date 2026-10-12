// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.herocubeworlds.generator.facets.CaveFacet;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * How big the open spaces actually are, in all three directions at once.
 * <p>
 * <b>A tall column of air is a chimney, not a hall.</b> Measuring the longest unbroken vertical run
 * finds the places where a gallery happens to stand on end, and says nothing about whether anything
 * fits beside it. This erodes the carved set instead: a cell survives {@code k} rounds of six-neighbour
 * erosion exactly when every cell within {@code k} steps of it is carved too, so surviving at {@code k}
 * means an open space at least {@code 2k+1} across in every axis direction. That is the number a
 * player feels as "a room".
 */
class RoomSizeProbeTest {

    private static final int FACE = 1;
    private static final int SPAN = 768;
    private static final int STEP = 256;
    private static final int WIDE = 96;
    private static final int LOWEST = -192;
    private static final int TALL = 256;
    private static final int ROUNDS = 26;

    @Test
    void theRoomsAreAsWideAsTheyAreTall() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry));
        builder.setSeed(0xC0FFEE);
        World world = builder.build();

        long[] survivors = new long[ROUNDS + 1];
        int[] bestAt = new int[3];
        int best = 0;
        int[] centre = geometry.faceCentre(FACE);

        for (int du = -SPAN; du <= SPAN; du += STEP) {
            for (int dv = -SPAN; dv <= SPAN; dv += STEP) {
                int x = centre[0] + du;
                int z = centre[1] + dv;
                if (geometry.faceOf(x, z) == CubePattern.NO_FACE) {
                    continue;
                }
                BlockRegion box = new BlockRegion(x, LOWEST, z).setSize(WIDE, TALL, WIDE);
                Region region = world.getWorldData(box);
                CaveFacet caves = region.getFacet(CaveFacet.class);

                boolean[] set = new boolean[WIDE * TALL * WIDE];
                for (int iz = 0; iz < WIDE; iz++) {
                    for (int iy = 0; iy < TALL; iy++) {
                        for (int ix = 0; ix < WIDE; ix++) {
                            set[index(ix, iy, iz)] = caves.getWorld(x + ix, LOWEST + iy, z + iz);
                        }
                    }
                }
                for (int i = 0; i < set.length; i++) {
                    if (set[i]) {
                        survivors[0]++;
                    }
                }
                for (int round = 1; round <= ROUNDS; round++) {
                    set = erode(set);
                    int count = 0;
                    for (int iz = 0; iz < WIDE; iz++) {
                        for (int iy = 0; iy < TALL; iy++) {
                            for (int ix = 0; ix < WIDE; ix++) {
                                if (set[index(ix, iy, iz)]) {
                                    count++;
                                    if (round > best) {
                                        best = round;
                                        bestAt[0] = x + ix;
                                        bestAt[1] = LOWEST + iy;
                                        bestAt[2] = z + iz;
                                    }
                                }
                            }
                        }
                    }
                    survivors[round] += count;
                    if (count == 0) {
                        break;
                    }
                }
            }
        }

        System.out.printf("carved cells sampled: %d%n", survivors[0]);
        System.out.println("open space that admits a ball of radius k (so 2k+1 across every way):");
        for (int k = 1; k <= ROUNDS; k++) {
            System.out.printf("  k=%d  (%2d across)  %8d cells  %.3f %% of the carved%n",
                    k, 2 * k + 1, survivors[k], 100.0 * survivors[k] / Math.max(1, survivors[0]));
        }
        System.out.printf("largest open space found: %d across, at %d, %d, %d%n",
                2 * best + 1, bestAt[0], bestAt[1], bestAt[2]);

        assertTrue(survivors[0] > 0, "nothing was carved in the sample at all");
    }

    /** One round of six-neighbour erosion: a cell survives only if all six neighbours were set. */
    private static boolean[] erode(boolean[] set) {
        boolean[] next = new boolean[set.length];
        for (int iz = 1; iz < WIDE - 1; iz++) {
            for (int iy = 1; iy < TALL - 1; iy++) {
                for (int ix = 1; ix < WIDE - 1; ix++) {
                    int at = index(ix, iy, iz);
                    next[at] = set[at]
                            && set[index(ix - 1, iy, iz)] && set[index(ix + 1, iy, iz)]
                            && set[index(ix, iy - 1, iz)] && set[index(ix, iy + 1, iz)]
                            && set[index(ix, iy, iz - 1)] && set[index(ix, iy, iz + 1)];
                }
            }
        }
        return next;
    }

    private static int index(int ix, int iy, int iz) {
        return ix + WIDE * (iy + TALL * iz);
    }
}
