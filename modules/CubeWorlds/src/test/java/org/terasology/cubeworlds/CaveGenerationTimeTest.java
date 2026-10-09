// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.generator.facets.CaveFacet;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.DensityFacet;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the caves cost in time, which is the only figure that answers "will this run on my machine".
 * <p>
 * {@code SphereFieldLatticeTest} counts calls into the noise, and that is a proxy: it is blind to
 * the interpolations, which happen once per field per cell of solid rock and are the part that
 * grows with a full chunk rather than with the lattice. So this one holds a stopwatch, against a
 * chain that is identical but for the one provider.
 * <p>
 * A wall clock in a test is a crude instrument and the bound is set loose on purpose — it is there
 * to catch a pass that has become tens of times more expensive, not to police a few per cent.
 */
class CaveGenerationTimeTest {

    private static final int CHUNKS = 24;

    @Test
    void carvingDoesNotDominateGeneration() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        World without = build(geometry, false);
        World with = build(geometry, true);

        // Warm the compiler on both before either is timed, or the first measured is the one that
        // pays for the JIT and the comparison says nothing.
        generate(without, geometry, 8, false);
        generate(with, geometry, 8, true);

        long plainStart = System.nanoTime();
        generate(without, geometry, CHUNKS, false);
        long plain = System.nanoTime() - plainStart;

        long carvedStart = System.nanoTime();
        generate(with, geometry, CHUNKS, true);
        long carved = System.nanoTime() - carvedStart;

        double plainMs = plain / 1e6 / CHUNKS;
        double carvedMs = carved / 1e6 / CHUNKS;
        System.out.printf("%.2f ms per chunk without the caves, %.2f ms with, %.2f ms of carving "
                        + "(%.0f %% more)%n",
                plainMs, carvedMs, carvedMs - plainMs, 100 * (carvedMs - plainMs) / plainMs);

        assertTrue(carvedMs < 6 * plainMs,
                "carving costs " + carvedMs + " ms a chunk against " + plainMs + " for the rest of "
                        + "the chain put together, which is no longer a pass over the terrain");
        assertTrue(carvedMs - plainMs < 20.0,
                "carving alone costs " + (carvedMs - plainMs) + " ms a chunk, which a machine "
                        + "loading a few hundred chunks will feel");
    }

    private static World build(CubeGeometry cube, boolean withCaves) {
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(cube.seaLevel())
                .addProvider(new SeaLevelProvider(cube.seaLevel()))
                .addProvider(new CubeSurfaceProvider(cube))
                .addProvider(new CubeClimateProvider(cube))
                .addProvider(new CubeRegionProvider(cube))
                .addProvider(new SurfaceToDensityProvider());
        if (withCaves) {
            builder.addProvider(new CubeCaveProvider(cube));
        }
        builder.setSeed(0xC0FFEE);
        return builder.build();
    }

    /** Pulls the same facets a rasterizer would, so nothing stays lazily unevaluated. */
    private static void generate(World target, CubeGeometry cube, int chunks, boolean withCaves) {
        int edge = cube.faceBlocks();
        int x = CubePattern.netX(1, edge / 2, edge);
        int z = CubePattern.netY(1, edge / 2, edge);
        for (int i = 0; i < chunks; i++) {
            BlockRegion request = new BlockRegion(x + (i % 6) * 32, -64 + (i / 6) * 64, z + (i % 4) * 32)
                    .setSize(32, 64, 32);
            Region region = target.getWorldData(request);
            DensityFacet density = region.getFacet(DensityFacet.class);
            if (density.getInternal().length == 0) {
                throw new IllegalStateException("nothing was generated");
            }
            if (withCaves) {
                CaveFacet caves = region.getFacet(CaveFacet.class);
                if (caves.getInternal().length == 0) {
                    throw new IllegalStateException("the cave facet is empty, so nothing was timed");
                }
            }
        }
    }
}
