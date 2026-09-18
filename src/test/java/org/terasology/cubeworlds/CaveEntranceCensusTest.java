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
import org.terasology.engine.world.generation.facets.ElevationFacet;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the caves can be walked into, and whether the sea stays out.
 * <p>
 * A network nobody can enter is the same as no network at all, and its opposite — a surface pitted
 * with holes everywhere a gallery runs shallow — is worse, because it is also ugly. The permission
 * to break the surface is a field of its own for exactly that reason, so this counts what the field
 * actually permits rather than what it was meant to.
 * <p>
 * The coordinates of the first few mouths are printed on purpose: they are where to stand in the
 * running game to look at the result.
 */
class CaveEntranceCensusTest {

    /** How close to the surface a gallery has to come to count as a way in. */
    private static final int REACH = 3;

    @Test
    void thereAreWaysInOnLandAndNoneUnderTheSea() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry));
        builder.setSeed(0xC0FFEE);
        World world = builder.build();

        int edge = geometry.faceBlocks();
        int landColumns = 0;
        int openLandColumns = 0;
        int seaColumns = 0;
        int openSeaColumns = 0;
        List<String> mouths = new ArrayList<>();

        // A grid of samples rather than a handful, and it took a wrong reading to see why: a mouth
        // is a patch a hundred blocks across, so a scattering of small windows either lands on one
        // or does not, and the figure that comes out measures the luck of the scattering rather
        // than the rate. Two settings a fifth apart read 1.11 % and 0.00 % on eight windows a face.
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int step = 0; step < 25; step++) {
                int u = edge / 6 * (step % 5 + 1) + (step / 5) * 37;
                int v = edge / 6 * (step / 5 + 1) + (step % 5) * 37;
                int x = CubePattern.netX(face, u, edge);
                int z = CubePattern.netY(face, v, edge);
                BlockRegion request = new BlockRegion(x, -64, z).setSize(16, 128, 16);
                Region region = world.getWorldData(request);
                ElevationFacet elevation = region.getFacet(ElevationFacet.class);
                CaveFacet caves = region.getFacet(CaveFacet.class);

                for (int dx = 0; dx < 16; dx++) {
                    for (int dz = 0; dz < 16; dz++) {
                        int cx = x + dx;
                        int cz = z + dz;
                        int top = (int) Math.floor(elevation.getWorld(cx, cz));
                        if (top < -63 || top > 62) {
                            continue;
                        }
                        boolean open = false;
                        for (int y = top; y > top - REACH; y--) {
                            if (caves.getWorld(cx, y, cz)) {
                                open = true;
                                break;
                            }
                        }
                        if (top > geometry.seaLevel()) {
                            landColumns++;
                            if (open) {
                                openLandColumns++;
                                if (mouths.size() < 5) {
                                    mouths.add("x=" + cx + " y=" + top + " z=" + cz);
                                }
                            }
                        } else {
                            seaColumns++;
                            if (open) {
                                openSeaColumns++;
                            }
                        }
                    }
                }
            }
        }

        double rate = landColumns == 0 ? 0 : (double) openLandColumns / landColumns;
        System.out.printf("%d land columns, %d of them open within %d blocks of the surface "
                        + "(%.3f %%); %d sea columns, %d open%n",
                landColumns, openLandColumns, REACH, 100 * rate, seaColumns, openSeaColumns);
        System.out.println("  first mouths: " + String.join(" | ", mouths));

        assertTrue(landColumns > 2000, "too little dry land was sampled for this to mean anything");
        assertTrue(openLandColumns > 0,
                "not one gallery reaches daylight: the network cannot be entered, so it may as "
                        + "well not exist");
        // A bound that bites. A mouth is a few columns wide, so a rate of one per cent already puts
        // one every twenty blocks or so and the ground reads as pitted rather than as a landscape
        // with caves in it; the first pass measured 1.11 % and looked exactly like that.
        assertTrue(rate < 0.008,
                "the ground is pitted rather than opened: " + (100 * rate) + " % of land columns "
                        + "have a hole in them");
        assertEquals(0, openSeaColumns,
                "a gallery broke through the sea floor: the seal below the ocean is not holding, "
                        + "and the hole would be a dry shaft under the water");
    }
}
