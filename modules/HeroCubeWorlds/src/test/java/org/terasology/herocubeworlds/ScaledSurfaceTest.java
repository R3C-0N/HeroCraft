// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.DensityFacet;
import org.terasology.engine.world.generation.facets.SurfaceTemperatureFacet;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The ground, asked for at a coarser scale, stays where it is.
 * <p>
 * A scaled region counts in cells of {@code scale} blocks, and every link of the chain has to agree
 * on what its facets hold. The stock density provider reads the elevation in blocks; the surface
 * provider used to hand it blocks divided by the scale, so a column asked for at scale four had its
 * ground at a quarter of its height — below the sea everywhere, which is what the old
 * level-of-detail drew when it was switched on: an ocean to the horizon.
 * <p>
 * Measured on the ground itself, the highest solid cell, and not on a facet value: a facet can be
 * right in its own units and wrong for whoever reads it.
 */
class ScaledSurfaceTest {

    private static final int[] SCALES = {1, 2, 4, 8};

    private static CubeGeometry geometry;
    private static World world;

    @BeforeAll
    static void buildWorld() {
        geometry = CubeGeometry.createDefault();
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider());
        builder.setSeed(0xC0FFEE);
        world = builder.build();
    }

    @Test
    void theGroundStaysPutAtEveryScale() {
        int[] centre = geometry.faceCentre(1);
        int span = geometry.faceBlocks() / 3;
        int samples = 12;
        System.out.printf("%6s %10s %10s %10s %10s%n", "echelle", "ecart moy", "ecart max", "biais", "temp. max");
        for (int scale : SCALES) {
            double sumAbs = 0;
            double sumSigned = 0;
            double worst = 0;
            double worstTemperature = 0;
            int n = 0;
            for (int i = 0; i < samples; i++) {
                for (int j = 0; j < samples; j++) {
                    // On the scaled lattice, so that the scaled cell and the full column are the same
                    // column of the world and not two neighbours.
                    int x = Math.floorDiv(centre[0] - span / 2 + i * span / samples, scale) * scale;
                    int z = Math.floorDiv(centre[1] - span / 2 + j * span / samples, scale) * scale;
                    int truth = groundAt(x, z, 1);
                    int scaled = groundAt(x, z, scale);
                    double error = scaled - truth;
                    sumAbs += Math.abs(error);
                    sumSigned += error;
                    worst = Math.max(worst, Math.abs(error));
                    worstTemperature = Math.max(worstTemperature,
                            Math.abs(temperatureAt(x, z, scale) - temperatureAt(x, z, 1)));
                    n++;
                }
            }
            System.out.printf("%6d %10.2f %10.1f %10.2f %10.4f%n", scale, sumAbs / n, worst, sumSigned / n,
                    worstTemperature);
            // One scaled cell is the resolution: the ground may land anywhere in it, never further.
            assertTrue(worst <= scale, "at scale " + scale + " the ground moved by " + worst + " blocks");
            assertEquals(0.0, worstTemperature, 1e-4, "the climate reads a different altitude at scale " + scale);
        }
    }

    /** The world height of the highest solid cell of a column, read at a given scale. */
    private static int groundAt(int x, int z, int scale) {
        int bottom = Math.floorDiv(geometry.floorY(), scale);
        int top = Math.floorDiv(geometry.ceilingY(), scale);
        BlockRegion cells = new BlockRegion(x / scale, bottom, z / scale).setSize(1, top - bottom + 1, 1);
        Region region = world.getWorldData(cells, scale);
        DensityFacet density = region.getFacet(DensityFacet.class);
        Vector3i pos = new Vector3i();
        for (int y = top; y >= bottom; y--) {
            if (density.getWorld(pos.set(x / scale, y, z / scale)) > 0) {
                return y * scale;
            }
        }
        return Integer.MIN_VALUE;
    }

    private static float temperatureAt(int x, int z, int scale) {
        BlockRegion cells = new BlockRegion(x / scale, 0, z / scale).setSize(1, 1, 1);
        return world.getWorldData(cells, scale).getFacet(SurfaceTemperatureFacet.class).getWorld(x / scale, z / scale);
    }
}
