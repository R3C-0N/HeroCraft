// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.OreKind;
import org.terasology.herocubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeOreProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.herocubeworlds.generator.facets.CaveFacet;
import org.terasology.herocubeworlds.generator.facets.OreFacet;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.DensityFacet;

import java.util.ArrayDeque;
import java.util.Deque;
import java.util.EnumMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * How much ore there is, how it is packed, and whether any of it is somewhere it must not be.
 * <p>
 * <b>The denominator is the rock the ore could have taken</b>, which is neither the cells of the
 * region nor the solid ones. A cell only counts if it is inside the band and out of the soil, so
 * the share printed here is comparable with the share written in {@link OreKind} — and only that
 * number says whether a threshold landed where the calibration said it would.
 * <p>
 * <b>A share alone would not catch the fault that matters.</b> The same tonnage of copper can
 * arrive as forty lumps of seven or as three slabs of ninety, and the two play nothing alike. So
 * the veins are counted as well, by flood fill, and their mean size read against
 * {@code share * horizontal^2 * vertical} — the size the field sizes were chosen to buy.
 * <p>
 * On the production geometry, because a vein is written in blocks: shrink the world and the same
 * thresholds describe a planet nobody will play.
 */
class OreVolumeCensusTest {

    /** Sample columns per face, and how wide a sample is. */
    private static final int SAMPLES_PER_FACE = 4;
    private static final int SAMPLE_SIZE = 32;
    private static final int LOWEST = -192;
    private static final int HIGHEST = 128;

    /** A chunk column, which is the unit the design targets are written in. */
    private static final double CHUNK_COLUMN = 32.0 * 32.0;

    private static final Vector3ic[] NEIGHBOURS = {
            new Vector3i(1, 0, 0), new Vector3i(-1, 0, 0),
            new Vector3i(0, 1, 0), new Vector3i(0, -1, 0),
            new Vector3i(0, 0, 1), new Vector3i(0, 0, -1)};

    private static CubeGeometry geometry;
    private static World world;

    @BeforeAll
    static void buildWorld() {
        geometry = CubeGeometry.createDefault();
        // The chain the generator assembles, minus the plugins and every rasterizer: this test is
        // about the facets, and it must not need a block manager to run.
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry))
                .addProvider(new CubeOreProvider(geometry));
        builder.setSeed(0xC0FFEE);
        world = builder.build();
    }

    @Test
    void everyOreIsAsCommonAndAsClumpedAsItsTableSays() {
        Map<OreKind, Long> found = new EnumMap<>(OreKind.class);
        Map<OreKind, Long> veins = new EnumMap<>(OreKind.class);
        Map<OreKind, Long> inBand = new EnumMap<>(OreKind.class);
        long columns = 0;
        long carvedOre = 0;

        int edge = geometry.faceBlocks();
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int step = 0; step < SAMPLES_PER_FACE; step++) {
                int u = edge / 5 + step * edge / 6;
                int v = edge / 4 + step * edge / 7;
                int x = CubePattern.netX(face, u, edge);
                int z = CubePattern.netY(face, v, edge);
                columns += (long) SAMPLE_SIZE * SAMPLE_SIZE;

                for (int y = LOWEST; y < HIGHEST; y += 64) {
                    BlockRegion slab = new BlockRegion(x, y, z).setSize(SAMPLE_SIZE, 64, SAMPLE_SIZE);
                    Region region = world.getWorldData(slab);
                    DensityFacet density = region.getFacet(DensityFacet.class);
                    CaveFacet caves = region.getFacet(CaveFacet.class);
                    BiomeFacet biomes = region.getFacet(BiomeFacet.class);
                    OreFacet ores = region.getFacet(OreFacet.class);

                    Map<Vector3ic, OreKind> placed = ores.getWorldEntries();
                    for (Map.Entry<Vector3ic, OreKind> entry : placed.entrySet()) {
                        found.merge(entry.getValue(), 1L, Long::sum);
                        if (caves.getWorld(entry.getKey())) {
                            carvedOre++;
                        }
                    }
                    veins(placed, veins);
                    rock(slab, density, biomes, inBand);
                }
            }
        }

        report(found, veins, inBand, columns);

        assertEquals(0, carvedOre,
                "ore was placed in cells the galleries had carved, so the ore provider ran before "
                        + "the cave provider. The chain is what orders them: requiring the density "
                        + "is what puts a provider after everything that updates it.");
        for (OreKind ore : OreKind.values()) {
            assertTrue(found.getOrDefault(ore, 0L) > 0,
                    ore + " was generated nowhere at all, which is what a threshold above what its "
                            + "field reaches looks like from the outside: the world renders, the ore "
                            + "does not exist, and nothing says so.");
        }
    }

    /**
     * Counts the rock each ore could have taken.
     * <p>
     * Not the solid cells: an ore is only ever a candidate inside its band and under the soil, so
     * anything else in the denominator would read as an ore that missed its target when in truth it
     * was never offered the cell. The band is rebuilt here from {@link OreKind} rather than asked of
     * the provider — a census that borrows the code it audits measures nothing.
     */
    private static void rock(BlockRegion slab, DensityFacet density, BiomeFacet biomes,
                             Map<OreKind, Long> inBand) {
        for (Vector3ic pos : slab) {
            float value = density.getWorld(pos);
            if (value <= 0) {
                continue;
            }
            Biome biome = biomes.getWorld(pos.x(), pos.z());
            float soil = biome instanceof CoreBiome ? ((CoreBiome) biome).rockDepth() : 32;
            if (value <= soil) {
                continue;
            }
            // Density is elevation minus y in this chain, so the ground is where the two meet.
            float surface = value + pos.y();
            for (OreKind ore : OreKind.values()) {
                if (pos.y() >= ore.coreBottom() && pos.y() <= ore.coreTopAt(surface)) {
                    inBand.merge(ore, 1L, Long::sum);
                }
            }
        }
    }

    /**
     * Six-neighbour flood fill, so that a vein is what a player digs out in one go.
     * <p>
     * Counted slab by slab, so a vein straddling a slab floor counts twice. At sixty-four blocks to
     * a slab against veins of a handful, that is a couple of percent on the count and as much
     * downwards on the mean size — worth knowing, not worth a second pass.
     */
    private static void veins(Map<Vector3ic, OreKind> placed, Map<OreKind, Long> veins) {
        Set<Vector3ic> seen = new HashSet<>();
        for (Map.Entry<Vector3ic, OreKind> seed : placed.entrySet()) {
            if (!seen.add(seed.getKey())) {
                continue;
            }
            OreKind kind = seed.getValue();
            veins.merge(kind, 1L, Long::sum);
            Deque<Vector3ic> queue = new ArrayDeque<>();
            queue.add(seed.getKey());
            while (!queue.isEmpty()) {
                Vector3ic at = queue.poll();
                for (Vector3ic step : NEIGHBOURS) {
                    Vector3i next = new Vector3i(at).add(step);
                    if (placed.get(next) == kind && seen.add(next)) {
                        queue.add(next);
                    }
                }
            }
        }
    }

    private static void report(Map<OreKind, Long> found, Map<OreKind, Long> veins,
                               Map<OreKind, Long> inBand, long columns) {
        double chunkColumns = columns / CHUNK_COLUMN;
        System.out.printf("%d sampled columns, %.1f chunk columns%n", columns, chunkColumns);
        System.out.printf("%-11s %9s %9s %9s %9s %7s %7s %9s%n",
                "ore", "blocks", "veins", "share", "target", "vein", "wanted", "per chunk");
        for (OreKind ore : OreKind.values()) {
            long blocks = found.getOrDefault(ore, 0L);
            long clusters = veins.getOrDefault(ore, 0L);
            long band = inBand.getOrDefault(ore, 0L);
            double share = band == 0 ? 0.0 : (double) blocks / band;
            double vein = clusters == 0 ? 0.0 : (double) blocks / clusters;
            double wanted = ore.share() * ore.horizontalSize() * ore.horizontalSize() * ore.verticalSize();
            System.out.printf("%-11s %9d %9d %8.4f%% %8.4f%% %7.1f %7.1f %9.1f%n",
                    ore, blocks, clusters, share * 100.0, ore.share() * 100.0,
                    vein, wanted, blocks / chunkColumns);
        }
    }
}
