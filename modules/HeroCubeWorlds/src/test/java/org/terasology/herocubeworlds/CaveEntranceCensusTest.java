// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.herocubeworlds.generator.CubeGeometry;
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

        // A grid of samples rather than a handful, and it took two wrong readings to size it. A
        // mouth is a patch a hundred blocks across, so a scattering of small windows either lands
        // on one or does not, and the figure that comes out measures the luck of the scattering
        // rather than the rate: two settings a fifth apart read 1.11 % and 0.00 % on eight windows
        // a face. Twenty-five windows of sixteen columns was still not enough — it read 0.76 %
        // where a contiguous survey of two million columns read 0.13 %, one porch of eighty
        // columns being half a per cent on its own. A window is a whole chunk now, which is four
        // times the ground for the same number of facet requests.
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int step = 0; step < 25; step++) {
                int u = edge / 6 * (step % 5 + 1) + (step / 5) * 37;
                int v = edge / 6 * (step / 5 + 1) + (step % 5) * 37;
                int x = CubePattern.netX(face, u, edge);
                int z = CubePattern.netY(face, v, edge);
                BlockRegion request = new BlockRegion(x, -64, z).setSize(32, 128, 32);
                Region region = world.getWorldData(request);
                ElevationFacet elevation = region.getFacet(ElevationFacet.class);
                CaveFacet caves = region.getFacet(CaveFacet.class);

                for (int dx = 0; dx < 32; dx++) {
                    for (int dz = 0; dz < 32; dz++) {
                        int cx = x + dx;
                        int cz = z + dz;
                        int top = (int) Math.floor(elevation.getWorld(cx, cz));
                        if (top < -63 || top > 62) {
                            continue;
                        }
                        // The surface cell itself, and the one under it. Scanning three cells down
                        // and taking any of them counted a gallery running two blocks below intact
                        // grass as a way in, which it is not — the assertion below says "have a
                        // hole in them" and that reading had no hole in it. It cost nothing while
                        // the mouths were points; the day the porches were widened it read 1,5 %
                        // and called a world pitted whose true opening rate was 0,15 %. Two cells
                        // rather than one, for the same reason CaveReachTest takes two: a single
                        // carved cell at the ground line is a dimple, not an entrance.
                        boolean open = caves.getWorld(cx, top, cz) && caves.getWorld(cx, top - 1, cz);
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
        System.out.printf("%d land columns, %d of them holed at the surface "
                        + "(%.3f %%); %d sea columns, %d open%n",
                landColumns, openLandColumns, 100 * rate, seaColumns, openSeaColumns);
        System.out.println("  first mouths: " + String.join(" | ", mouths));

        assertTrue(landColumns > 2000, "too little dry land was sampled for this to mean anything");
        assertTrue(openLandColumns > 0,
                "not one gallery reaches daylight: the network cannot be entered, so it may as "
                        + "well not exist");
        // A bound of taste, and it is inherited: the 1.11 % that set it was measured with the
        // any-of-three-cells rule this test no longer uses, so the figure and the quantity do not
        // match any more. It has not been re-derived by eye against the new rule, and the reason it
        // has not is that nothing has come near it: the flare settings tried read 0,046 % at 1.0,
        // 0,174 % at 1.7 and 0,659 % at 3.2, so the bound separates settings even if the exact
        // place it sits is not defended. Re-derive it the day a setting lands between 0,4 and 0,8.
        assertTrue(rate < 0.008,
                "the ground is pitted rather than opened: " + (100 * rate) + " % of land columns "
                        + "have a hole in them");
        assertEquals(0, openSeaColumns,
                "a gallery broke through the sea floor: the seal below the ocean is not holding, "
                        + "and the hole would be a dry shaft under the water");
    }
}
