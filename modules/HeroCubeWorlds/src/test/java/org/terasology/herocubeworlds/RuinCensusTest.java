// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.RuinCulture;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.herocubeworlds.generator.RuinShape;
import org.terasology.herocubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRuinProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.herocubeworlds.generator.facets.RuinFacet;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.ElevationFacet;

import java.util.EnumMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * How many ruins the planet carries, of which cultures, and whether each stands where the rule says
 * — dry, flat enough, and judged alike by every chunk it touches.
 * <p>
 * The coordinates of the first ruins are printed on purpose: they are where to go in the running
 * game to look at one.
 */
class RuinCensusTest {

    @Test
    void ruinsStandOnDryFlatSettledGroundAndEveryChunkAgrees() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        CubeRuinProvider ruins = new CubeRuinProvider(geometry, CubeWorldGenerator::columnProviders);
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(ruins);
        builder.setSeed(0xC0FFEE);
        World world = builder.build();

        int edge = geometry.faceBlocks();
        int side = edge * CubePattern.PATTERN_SIDE;
        List<RuinLattice.Site> sites = ruins.lattice().sitesNear(0, 0, side - 1, side - 1, 0);
        Map<RuinCulture, Integer> cultures = new EnumMap<>(RuinCulture.class);
        int[] perFace = new int[CubePattern.FACES];
        int total = 0;
        StringBuilder first = new StringBuilder();

        for (RuinLattice.Site site : sites) {
            RuinFacet.Ruin ruin = ruinAt(world, geometry, site, site.x(), site.z());
            if (ruin == null) {
                continue;
            }
            total++;
            perFace[site.face()]++;
            cultures.merge(ruin.culture(), 1, Integer::sum);
            if (total <= 5) {
                first.append(String.format(" | %s x=%d y=%d z=%d", ruin.culture(), site.x(), ruin.floorY(), site.z()));
            }

            // A chunk that holds only the far corner of the footprint reaches the same verdict.
            int corner = RuinShape.RADIUS;
            RuinFacet.Ruin seen = ruinAt(world, geometry, site, site.x() + corner, site.z() - corner);
            assertTrue(seen != null && seen.floorY() == ruin.floorY() && seen.culture() == ruin.culture(),
                    "a corner of the ruin at " + site.x() + "," + site.z() + " was judged otherwise");

            BlockRegion footprint = new BlockRegion(site.x() - corner, 0, site.z() - corner)
                    .setSize(2 * corner + 1, 1, 2 * corner + 1);
            ElevationFacet elevation = world.getWorldData(footprint).getFacet(ElevationFacet.class);
            int lowest = Integer.MAX_VALUE;
            int highest = Integer.MIN_VALUE;
            for (int dz = -corner; dz <= corner; dz++) {
                for (int dx = -corner; dx <= corner; dx++) {
                    int top = CubeRuinProvider.topOf(elevation.getWorld(site.x() + dx, site.z() + dz));
                    lowest = Math.min(lowest, top);
                    highest = Math.max(highest, top);
                }
            }
            // The verdict is taken on a private copy of the column chain; this is the world's own.
            assertEquals(CubeRuinProvider.topOf(elevation.getWorld(site.x(), site.z())), ruin.floorY(),
                    "the ruin was judged on other ground than the world lays");
            assertTrue(lowest >= geometry.seaLevel() + CubeRuinProvider.DRY_MARGIN, "a ruin stands in the sea");
            assertTrue(highest - lowest <= CubeRuinProvider.MAX_RELIEF, "a ruin stands on a cliff");
        }

        System.out.printf("%d ruins on %d candidate sites; per face %s; cultures %s%n",
                total, sites.size(), java.util.Arrays.toString(perFace), cultures);
        System.out.println("  first ruins:" + first);

        assertTrue(total >= 30, "fewer than thirty ruins on the planet: the land is empty of the past");
        for (int face = 0; face < CubePattern.FACES; face++) {
            if (face != CubePattern.NORTH && face != CubePattern.SOUTH) {
                assertTrue(perFace[face] > 0, "face " + face + " carries no ruin at all");
            }
        }
        assertTrue(cultures.size() >= 3, "the ruins are all of one or two peoples: " + cultures);
    }

    /** The ruin the provider reports at a site, from a region holding only the given column. */
    private static RuinFacet.Ruin ruinAt(World world, CubeGeometry geometry, RuinLattice.Site site, int x, int z) {
        Region region = world.getWorldData(new BlockRegion(x, geometry.floorY(), z)
                .setSize(1, geometry.ceilingY() - geometry.floorY() + 1, 1));
        for (RuinFacet.Ruin ruin : region.getFacet(RuinFacet.class).getRuins()) {
            if (ruin.site().x() == site.x() && ruin.site().z() == site.z()) {
                assertEquals(site.seed(), ruin.site().seed());
                return ruin;
            }
        }
        return null;
    }
}
