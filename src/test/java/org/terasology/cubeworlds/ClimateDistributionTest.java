// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector2i;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereSampler;
import org.terasology.cubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generation.facets.SurfaceHumidityFacet;
import org.terasology.engine.world.generation.facets.SurfaceTemperatureFacet;

import java.util.Arrays;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The distribution of the climate, because no threshold can be guessed.
 * <p>
 * A biome threshold set a priori is a threshold set against an imagined distribution. This prints
 * the real quantiles, and asserts that the fields stay spread out: a climate that collapses towards
 * its middle opens no biome at all, and does so without any error.
 */
class ClimateDistributionTest {

    private static final int FACE_CHUNKS = 16;

    private static CubeGeometry geometry;
    private static World world;

    @BeforeAll
    static void buildWorld() {
        geometry = new CubeGeometry(FACE_CHUNKS, CubeGeometry.DEFAULT_SEA_LEVEL, ConformalCubeTable.build());
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeBiomeProvider());
        builder.setSeed(0xC0FFEE);
        world = builder.build();
    }

    private static void quantiles(String name, double[] values) {
        double[] sorted = values.clone();
        Arrays.sort(sorted);
        System.out.printf("%-12s min %8.3f  q05 %8.3f  q25 %8.3f  med %8.3f  q75 %8.3f  q95 %8.3f  max %8.3f%n",
                name, at(sorted, 0.0), at(sorted, 0.05), at(sorted, 0.25), at(sorted, 0.5),
                at(sorted, 0.75), at(sorted, 0.95), at(sorted, 1.0));
    }

    private static double at(double[] sorted, double q) {
        int i = (int) Math.round(q * (sorted.length - 1));
        return sorted[i];
    }

    @Test
    void theClimateStaysSpreadOut() {
        int edge = geometry.faceBlocks();
        int stride = edge / 20;
        int margin = edge / 8;
        int n = 0;
        double[] temp = new double[4096];
        double[] humid = new double[4096];
        double[] elev = new double[4096];
        double[] latitude = new double[4096];
        double[] polarTemp = new double[4096];
        int polar = 0;

        for (int face = 0; face < CubePattern.FACES; face++) {
            SphereSampler sampler = new SphereSampler(geometry, face);
            for (int u = margin; u < edge - margin; u += stride) {
                for (int v = margin; v < edge - margin; v += stride) {
                    int x = CubePattern.netX(face, u, edge);
                    int z = CubePattern.netY(face, v, edge);
                    Region region = world.getWorldData(
                            new BlockRegion(x, geometry.seaLevel(), z).setSize(1, 1, 1));
                    Vector2i origin = new Vector2i(0, 0);
                    double t = region.getFacet(SurfaceTemperatureFacet.class).get(origin);
                    sampler.moveTo(x, z);
                    temp[n] = t;
                    humid[n] = region.getFacet(SurfaceHumidityFacet.class).get(origin);
                    elev[n] = region.getFacet(ElevationFacet.class).get(origin) - geometry.seaLevel();
                    latitude[n] = Math.abs(sampler.sinLatitude());
                    if (latitude[n] > 0.9) {
                        polarTemp[polar++] = t;
                    }
                    n++;
                }
            }
        }
        temp = Arrays.copyOf(temp, n);
        humid = Arrays.copyOf(humid, n);
        elev = Arrays.copyOf(elev, n);
        latitude = Arrays.copyOf(latitude, n);
        polarTemp = Arrays.copyOf(polarTemp, polar);

        System.out.printf("%d columns%n", n);
        quantiles("temperature", temp);
        quantiles("humidity", humid);
        quantiles("elevation", elev);
        quantiles("|sin lat|", latitude);
        quantiles("polar temp", polarTemp);

        assertTrue(at(sorted(temp), 0.95) - at(sorted(temp), 0.05) > 0.25,
                "the temperature collapsed towards its middle, no threshold can open a biome");
        assertTrue(at(sorted(elev), 0.95) > 30, "the land never rises anywhere");
        assertTrue(polar > 0, "no polar column was sampled");
        // The claim that makes this a climate rather than a field of noise: the poles are colder
        // than the world, by a margin that a background field could not produce on its own.
        double polarMedian = at(sorted(polarTemp), 0.5);
        double worldMedian = at(sorted(temp), 0.5);
        System.out.printf("polar median %.3f against a world median of %.3f%n", polarMedian, worldMedian);
        assertTrue(worldMedian - polarMedian > 0.15,
                "latitude does not dominate the temperature: " + polarMedian + " against " + worldMedian);
    }

    private static double[] sorted(double[] values) {
        double[] copy = values.clone();
        Arrays.sort(copy);
        return copy;
    }
}
