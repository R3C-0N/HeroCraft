// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector2i;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereSampler;
import org.terasology.cubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generation.facets.SurfaceTemperatureFacet;

import java.util.EnumMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the world actually contains, asked of the real provider chain rather than of the formulas.
 * <p>
 * A threshold that opens onto nothing is a silent failure, so each biome that is expected to exist
 * is counted, and a playable coordinate is printed for it. And the claim that gives the climate its
 * shape — that latitude, not noise, decides where it is cold — is checked by where the snow ended
 * up, not by re-reading the code that put it there.
 */
class BiomeCensusTest {

    private static final int FACE_CHUNKS = 16;

    private static CubeGeometry geometry;
    private static World world;

    @BeforeAll
    static void buildWorld() {
        geometry = new CubeGeometry(FACE_CHUNKS, CubeGeometry.DEFAULT_SEA_LEVEL, ConformalCubeTable.build());
        // The same chain the generator assembles, minus the plugins and the rasterizers: this test
        // is about the facets, and it must not need a block manager to run.
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider());
        builder.setSeed(0xC0FFEE);
        world = builder.build();
    }

    private static Region columnAt(int x, int z) {
        return world.getWorldData(new BlockRegion(x, geometry.seaLevel(), z).setSize(1, 1, 1));
    }

    @Test
    void everyExpectedBiomeExistsSomewhere() {
        Map<CoreBiome, Integer> census = new EnumMap<>(CoreBiome.class);
        Map<CoreBiome, int[]> witness = new EnumMap<>(CoreBiome.class);
        int edge = geometry.faceBlocks();
        int stride = edge / 24;
        int margin = edge / 8;
        int total = 0;

        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int u = margin; u < edge - margin; u += stride) {
                for (int v = margin; v < edge - margin; v += stride) {
                    int x = CubePattern.netX(face, u, edge);
                    int z = CubePattern.netY(face, v, edge);
                    CoreBiome biome = (CoreBiome) columnAt(x, z).getFacet(BiomeFacet.class).get(new Vector2i(0, 0));
                    census.merge(biome, 1, Integer::sum);
                    witness.putIfAbsent(biome, new int[]{x, z});
                    total++;
                }
            }
        }

        System.out.printf("%d columns sampled over six faces of %d blocks%n", total, edge);
        for (CoreBiome biome : CoreBiome.values()) {
            int count = census.getOrDefault(biome, 0);
            int[] where = witness.get(biome);
            System.out.printf("  %-10s %5.1f %%  %s%n", biome.getDisplayName(), 100.0 * count / total,
                    where == null ? "nowhere" : "at x=" + where[0] + " z=" + where[1]);
        }

        // Every biome the cascade can reach. SKY and UNDERGROUND are deliberately absent: the
        // skies have no column and the deep underground is a volume rather than a column, so
        // nothing assigns them and nothing should expect them. Every other value is a threshold,
        // and a threshold nobody crosses is a silent failure.
        for (CoreBiome expected : CoreBiome.values()) {
            if (expected == CoreBiome.SKY || expected == CoreBiome.UNDERGROUND) {
                continue;
            }
            assertTrue(census.getOrDefault(expected, 0) > 0, expected.getDisplayName() + " exists nowhere");
        }

        assertEquals(0, census.getOrDefault(CoreBiome.SKY, 0), "the skies were given a column");
        assertEquals(0, census.getOrDefault(CoreBiome.UNDERGROUND, 0),
                "the deep underground was named on the surface");
    }

    /**
     * The claim of the climate, measured where it lands. If noise dominated instead, the snow would
     * be scattered and this average would sit near the average of the whole world.
     */
    @Test
    void theColdEndsUpAtThePoles() {
        int edge = geometry.faceBlocks();
        int stride = edge / 16;
        int margin = edge / 8;
        double snowLatitude = 0.0;
        int snowCount = 0;
        double allLatitude = 0.0;
        int allCount = 0;

        for (int face = 0; face < CubePattern.FACES; face++) {
            SphereSampler sampler = new SphereSampler(geometry, face);
            for (int u = margin; u < edge - margin; u += stride) {
                for (int v = margin; v < edge - margin; v += stride) {
                    int x = CubePattern.netX(face, u, edge);
                    int z = CubePattern.netY(face, v, edge);
                    sampler.moveTo(x, z);
                    double latitude = Math.abs(sampler.sinLatitude());
                    CoreBiome biome = (CoreBiome) columnAt(x, z).getFacet(BiomeFacet.class).get(new Vector2i(0, 0));
                    allLatitude += latitude;
                    allCount++;
                    if (biome == CoreBiome.SNOW) {
                        snowLatitude += latitude;
                        snowCount++;
                    }
                }
            }
        }

        double snowMean = snowLatitude / snowCount;
        double allMean = allLatitude / allCount;
        System.out.printf("snow sits at |sin latitude| %.3f on average, the world at %.3f (%d snow columns)%n",
                snowMean, allMean, snowCount);
        assertTrue(snowCount > 0, "no snow anywhere");
        assertTrue(snowMean > allMean + 0.2,
                "snow is not gathered towards the poles: " + snowMean + " against " + allMean);
    }

    @Test
    void aDeadSlotCarriesNoLand() {
        int edge = geometry.faceBlocks();
        // slot (0, 0) of the net is dead
        int x = edge / 2;
        int z = edge / 2;
        assertEquals(CubePattern.NO_FACE, geometry.faceOf(x, z), "picked a live slot by mistake");
        Region region = columnAt(x, z);
        float elevation = region.getFacet(ElevationFacet.class).get(new Vector2i(0, 0));
        float temperature = region.getFacet(SurfaceTemperatureFacet.class).get(new Vector2i(0, 0));
        System.out.printf("a dead slot reports elevation %.1f and temperature %.2f%n", elevation, temperature);
        assertTrue(elevation < geometry.seaLevel() - 50, "a dead slot should be far below the reference level");
        assertEquals(0.5f, temperature, 1e-6, "a dead slot should report a neutral climate");
    }
}
