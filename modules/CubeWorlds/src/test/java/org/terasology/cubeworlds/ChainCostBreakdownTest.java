// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeOreProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.FacetProvider;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.WorldFacet;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What each pass of the chain costs, measured rather than reasoned about.
 * <p>
 * {@code CaveGenerationTimeTest} times the chain with and without the caves and answers "do the
 * caves dominate". It cannot answer "what would a cache of the surface save", because everything
 * that is not the caves sits in one undivided figure — and inside that figure
 * {@link SurfaceToDensityProvider} fills 65 536 cells a chunk, which no cache of a heightmap would
 * remove. This test splits it.
 * <p>
 * The method is cumulative: chain <i>n</i> is chain <i>n-1</i> plus one provider, and every facet
 * the chain produces is pulled, so the whole of it runs. The marginal figure is the difference. That
 * is sound only because {@code RegionImpl.getFacet} is lazy — pulling everything is what makes the
 * comparison fair, and pulling one terminal facet would silently skip providers.
 */
class ChainCostBreakdownTest {

    private static final int CHUNKS = 48;
    private static final int WARMUP = 32;

    @Test
    void eachPassOfTheChainIsPricedSeparately() {
        CubeGeometry cube = CubeGeometry.createDefault();

        List<String> names = new ArrayList<>();
        List<Supplier<FacetProvider>> steps = new ArrayList<>();
        names.add("surface (elevation)");
        steps.add(() -> new CubeSurfaceProvider(cube));
        names.add("climat (temperature, humidite)");
        steps.add(() -> new CubeClimateProvider(cube));
        names.add("regions (4 masques + latitude)");
        steps.add(() -> new CubeRegionProvider(cube));
        names.add("biome");
        steps.add(() -> new CubeBiomeProvider());
        names.add("densite (elevation - y, 65536 cellules)");
        steps.add(() -> new SurfaceToDensityProvider());
        names.add("grottes (creusement)");
        steps.add(() -> new CubeCaveProvider(cube));
        names.add("minerais (filons)");
        steps.add(() -> new CubeOreProvider(cube));

        // Every chain is built and exercised before any of them is timed. Warming each chain just
        // before measuring it, which is the obvious way round, makes the first chain measured pay
        // for the JIT of the noise and the sampler that all the others then find already compiled:
        // it read 6,3 ms for the surface alone against 2,08 ms for a whole chain elsewhere, and two
        // of the marginal figures came out negative. A negative marginal cost is not a small
        // number, it is a broken measurement.
        List<World> worlds = new ArrayList<>();
        for (int upTo = 1; upTo <= steps.size(); upTo++) {
            worlds.add(build(cube, steps, upTo));
        }
        for (World world : worlds) {
            generate(world, cube, WARMUP);
        }

        double previous = 0.0;
        double total = 0.0;
        System.out.printf("%n%-42s %10s %10s%n", "passe ajoutee", "cumul ms", "marge ms");
        System.out.println("-".repeat(64));
        for (int upTo = 1; upTo <= steps.size(); upTo++) {
            World world = worlds.get(upTo - 1);
            long start = System.nanoTime();
            generate(world, cube, CHUNKS);
            double ms = (System.nanoTime() - start) / 1e6 / CHUNKS;
            System.out.printf("%-42s %10.3f %10.3f%n", names.get(upTo - 1), ms, ms - previous);
            previous = ms;
            total = ms;
        }
        System.out.printf("%n chaine complete : %.3f ms par chunk%n", total);

        assertTrue(total > 0.0, "nothing was timed at all");
    }

    private static World build(CubeGeometry cube, List<Supplier<FacetProvider>> steps, int upTo) {
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(cube.seaLevel())
                .addProvider(new SeaLevelProvider(cube.seaLevel()));
        for (int i = 0; i < upTo; i++) {
            builder.addProvider(steps.get(i).get());
        }
        builder.setSeed(0xC0FFEE);
        return builder.build();
    }

    /**
     * Pulls every facet the chain declares, so no provider stays lazily unevaluated. Anything less
     * would time a subset that changes from one chain to the next, and the marginal figures would be
     * differences between unlike things.
     */
    private static void generate(World target, CubeGeometry cube, int chunks) {
        int edge = cube.faceBlocks();
        int x = CubePattern.netX(1, edge / 2, edge);
        int z = CubePattern.netY(1, edge / 2, edge);
        for (int i = 0; i < chunks; i++) {
            BlockRegion request = new BlockRegion(x + (i % 8) * 32, -64 + (i / 8) * 64, z + (i % 4) * 32)
                    .setSize(32, 64, 32);
            Region region = target.getWorldData(request);
            for (Class<? extends WorldFacet> facet : target.getAllFacets()) {
                if (region.getFacet(facet) == null) {
                    throw new IllegalStateException("facet not produced: " + facet.getSimpleName());
                }
            }
        }
    }
}
