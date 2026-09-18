// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3ic;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.generator.facets.CaveFacet;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.DensityFacet;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * How much of the rock is gallery, band of depth by band of depth.
 * <p>
 * <b>The denominator is the rock, not the world.</b> Dividing carved cells by the cells of a band
 * would read near zero in the upper bands for no better reason than that there is barely any stone
 * up there, and it would look exactly like a number: a measurement of the geology, worn as a
 * measurement of the caves. What is counted here is cells that were solid before the pass ran.
 * <p>
 * On the production geometry, because a bore is written in blocks: on a small world the same
 * thresholds carve nearly everything, and every figure would describe a planet nobody will play.
 */
class CaveVolumeCensusTest {

    private static CubeGeometry geometry;
    private static World world;

    private static final int[] BAND_EDGES = {-192, -176, -160, -128, -96, -64, -32, 0, 32, 64, 128};

    @BeforeAll
    static void buildWorlds() {
        geometry = CubeGeometry.createDefault();
        world = build(geometry);
    }

    private static World build(CubeGeometry cube) {
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(cube.seaLevel())
                .addProvider(new SeaLevelProvider(cube.seaLevel()))
                .addProvider(new CubeSurfaceProvider(cube))
                .addProvider(new CubeClimateProvider(cube))
                .addProvider(new CubeRegionProvider(cube))
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(cube));
        builder.setSeed(0xC0FFEE);
        return builder.build();
    }

    @Test
    void theRockIsCarvedButNotHollowed() {
        long[] rock = new long[BAND_EDGES.length - 1];
        long[] carved = new long[BAND_EDGES.length - 1];
        census(world, rock, carved);

        long totalRock = 0;
        long totalCarved = 0;
        System.out.printf("%-16s %12s %10s%n", "band", "solid cells", "carved");
        for (int band = 0; band < rock.length; band++) {
            totalRock += rock[band];
            totalCarved += carved[band];
            System.out.printf("[%5d,%5d) %12d %9.2f %%%n",
                    BAND_EDGES[band], BAND_EDGES[band + 1], rock[band], 100.0 * fraction(rock, carved, band));
        }
        double overall = totalRock == 0 ? 0 : (double) totalCarved / totalRock;
        System.out.printf("overall: %.2f %% of %d solid cells%n", 100 * overall, totalRock);

        assertEquals(0, carved[0],
                "the mantle's cover was breached: something was carved in the sixteen blocks above "
                        + "the floor of the world, which a hard condition is supposed to forbid");
        assertTrue(totalRock > 200000, "too little rock was sampled for these figures to mean anything");
        assertTrue(overall > 0.012,
                "there are barely any caves: a hundred block shaft would meet one once in eleven "
                        + "tries and a player would conclude the world is solid. Carved " + overall);
        assertTrue(overall < 0.06,
                "the rock is a sponge rather than a set of galleries: carved " + overall);

        // The profile does something, and the claim is falsifiable: the deep bands are carved wider
        // than the shallow ones because the radius is scaled with depth, not because the noise
        // happens to be busier down there.
        double deep = weighted(rock, carved, -160, -96);
        double shallow = weighted(rock, carved, 0, 64);
        System.out.printf("deep [-160,-96) %.2f %% against shallow [0,64) %.2f %%%n",
                100 * deep, 100 * shallow);
        assertTrue(deep > 1.15 * shallow,
                "the depth profile is not doing anything: " + deep + " deep against " + shallow + " shallow");
    }

    private static void census(World target, long[] rock, long[] carved) {
        int edge = geometry.faceBlocks();
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int step = 0; step < 3; step++) {
                int u = edge / 4 + step * edge / 4;
                int v = edge / 3 + step * edge / 5;
                int x = CubePattern.netX(face, u, edge);
                int z = CubePattern.netY(face, v, edge);
                for (int y = -192; y < 128; y += 64) {
                    Region region = target.getWorldData(new BlockRegion(x, y, z).setSize(16, 64, 16));
                    DensityFacet density = region.getFacet(DensityFacet.class);
                    CaveFacet caves = region.getFacet(CaveFacet.class);
                    for (Vector3ic pos : new BlockRegion(x, y, z).setSize(16, 64, 16)) {
                        boolean hollow = caves.getWorld(pos);
                        boolean wasRock = hollow || density.getWorld(pos) > 0;
                        if (!wasRock) {
                            continue;
                        }
                        int band = bandOf(pos.y());
                        if (band >= 0) {
                            rock[band]++;
                            if (hollow) {
                                carved[band]++;
                            }
                        }
                    }
                }
            }
        }
    }

    private static int bandOf(int y) {
        for (int band = 0; band < BAND_EDGES.length - 1; band++) {
            if (y >= BAND_EDGES[band] && y < BAND_EDGES[band + 1]) {
                return band;
            }
        }
        return -1;
    }

    private static double fraction(long[] rock, long[] carved, int band) {
        return rock[band] == 0 ? 0.0 : (double) carved[band] / rock[band];
    }

    private static double weighted(long[] rock, long[] carved, int from, int to) {
        long totalRock = 0;
        long totalCarved = 0;
        for (int band = 0; band < rock.length; band++) {
            if (BAND_EDGES[band] >= from && BAND_EDGES[band + 1] <= to) {
                totalRock += rock[band];
                totalCarved += carved[band];
            }
        }
        return totalRock == 0 ? 0.0 : (double) totalCarved / totalRock;
    }
}
