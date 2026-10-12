// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;

import java.util.Locale;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The generator run by the real engine, through the real chunk pipeline.
 * <p>
 * Everything else in this module tests facets and formulas. This one is the only thing that can say
 * a block was actually placed, and the only thing that can catch the two failures the facets cannot
 * see: a chain that never assembles, and a dead slot of the net quietly filling with water.
 */
@IntegrationEnvironment(worldGenerator = "HeroCubeWorlds:cubeworld")
public class CubeWorldGenerationTest {

    @In
    private WorldProvider worldProvider;

    @In
    private MainLoop mainLoop;

    private static final int EDGE = CubeGeometry.DEFAULT_FACE_CHUNKS * 32;

    @Test
    public void theMiddleOfAFaceCarriesGround() {
        // the centre of face 1, where the world generator puts the spawn
        int x = (int) (1.5 * EDGE);
        int z = (int) (1.5 * EDGE);
        mainLoop.forceAndWaitForGeneration(new Vector3i(x, CubeGeometry.DEFAULT_SEA_LEVEL, z));

        int solid = 0;
        for (int y = 0; y < 64; y++) {
            Block block = worldProvider.getBlock(x, y, z);
            assertNotEquals("engine:unloaded", block.getURI().toString(),
                    "the column was never generated at y=" + y);
            if (!block.isPenetrable()) {
                solid++;
            }
        }
        System.out.printf("the column at the middle of a face has %d solid blocks out of 64%n", solid);
        assertTrue(solid > 0, "the middle of a face carries no ground at all");
    }

    /**
     * There is no companion test for the ceiling, and that is the finding rather than an omission:
     * the land tops out around fifty blocks above the reference level, so nothing is generated
     * anywhere near +319 and a test there would assert that air is air. The ceiling only means
     * something to the build limit, which is a rule about placing blocks and not about generating
     * them.
     */
    @Test
    public void theWorldEndsOnMantleStone() {
        int x = (int) (1.5 * EDGE);
        int z = (int) (1.5 * EDGE);
        // Below the floor first, so the chunk under the world is loaded too: asking for a block in
        // a chunk nobody asked for answers engine:unloaded, which would pass for emptiness.
        mainLoop.forceAndWaitForGeneration(new Vector3i(x, CubeGeometry.DEFAULT_FLOOR_Y - 1, z));
        mainLoop.forceAndWaitForGeneration(new Vector3i(x, CubeGeometry.DEFAULT_FLOOR_Y + 1, z));

        assertEquals("CoreAssets:MantleStone",
                worldProvider.getBlock(x, CubeGeometry.DEFAULT_FLOOR_Y, z).getURI().toString(),
                "the world does not end on mantle stone");
        assertEquals("engine:air",
                worldProvider.getBlock(x, CubeGeometry.DEFAULT_FLOOR_Y - 1, z).getURI().toString(),
                "there is still something below the floor of the world");
        // One layer and no more. The density down here is enormous and positive, so a floor written
        // as "everything below" would have turned the bottom chunk into a slab of mantle.
        assertNotEquals("CoreAssets:MantleStone",
                worldProvider.getBlock(x, CubeGeometry.DEFAULT_FLOOR_Y + 1, z).getURI().toString(),
                "the mantle is more than one layer thick");
    }

    /**
     * The one failure the facet tests cannot see, checked on real blocks.
     * <p>
     * The rasterizer answers an empty cell below the reference level with water, which is right for
     * an ocean. The sea sits at 32 and the floor of the world at -192, so without the cave facet
     * telling carved cells apart from sea floor, every gallery under 32 — very nearly all of them —
     * would fill. A census of facets would show the rock carved either way; only a block says which.
     */
    @Test
    public void aGalleryDoesNotFillWithWater() {
        int x0 = (int) (1.5 * EDGE);
        int z0 = (int) (1.5 * EDGE);
        for (int y = -160; y <= 32; y += 64) {
            mainLoop.forceAndWaitForGeneration(new Vector3i(x0, y, z0));
        }

        int landColumns = 0;
        int airUnderLand = 0;
        int waterUnderLand = 0;
        for (int x = x0; x < x0 + 16; x++) {
            for (int z = z0; z < z0 + 16; z++) {
                int top = topOf(x, z);
                if (top <= CubeGeometry.DEFAULT_SEA_LEVEL) {
                    continue;
                }
                landColumns++;
                for (int y = -160; y < Math.min(top, CubeGeometry.DEFAULT_SEA_LEVEL); y++) {
                    String uri = worldProvider.getBlock(x, y, z).getURI().toString().toLowerCase(Locale.ROOT);
                    if ("engine:air".equals(uri)) {
                        airUnderLand++;
                    } else if (uri.contains("water")) {
                        waterUnderLand++;
                    }
                }
            }
        }

        System.out.printf("%d land columns: %d cells of air and %d of water below the sea level "
                + "and below the ground%n", landColumns, airUnderLand, waterUnderLand);
        assertTrue(landColumns > 0, "no land was sampled here, so this proves nothing about caves");
        assertTrue(airUnderLand > 0,
                "there is not one empty cell under dry land: either nothing was carved, or the "
                        + "carved cells were all turned into something");
        assertEquals(0, waterUnderLand,
                "the galleries filled with water: the rasterizer took carved rock for sea floor");
    }

    /**
     * The lava is a level, so the test is that it is one: full below the line and absent above it,
     * everywhere, rather than a scattering of pools that happen to be deep.
     */
    @Test
    public void theDeepestGalleriesAreDrownedInLava() {
        int x0 = (int) (1.5 * EDGE);
        int z0 = (int) (1.5 * EDGE);
        mainLoop.forceAndWaitForGeneration(new Vector3i(x0, CubeGeometry.DEFAULT_LAVA_Y, z0));

        int below = 0;
        int above = 0;
        for (int x = x0; x < x0 + 16; x++) {
            for (int z = z0; z < z0 + 16; z++) {
                for (int y = CubeGeometry.DEFAULT_LAVA_Y - 16; y <= CubeGeometry.DEFAULT_LAVA_Y + 16; y++) {
                    if (worldProvider.getBlock(x, y, z).getURI().toString()
                            .toLowerCase(Locale.ROOT).contains("lava")) {
                        if (y <= CubeGeometry.DEFAULT_LAVA_Y) {
                            below++;
                        } else {
                            above++;
                        }
                    }
                }
            }
        }

        System.out.printf("%d cells of lava at or below y=%d, %d above it%n",
                below, CubeGeometry.DEFAULT_LAVA_Y, above);
        assertTrue(below > 0, "the deepest galleries are dry, so the bottom of the world is dark "
                + "and a screenshot of it says nothing");
        assertEquals(0, above, "the lava is not a level: some of it stands above the line");
    }

    /** @return the height of the highest non-penetrable block in a column, or the floor if none. */
    private int topOf(int x, int z) {
        for (int y = 128; y > CubeGeometry.DEFAULT_FLOOR_Y; y--) {
            if (!worldProvider.getBlock(x, y, z).isPenetrable()) {
                return y;
            }
        }
        return CubeGeometry.DEFAULT_FLOOR_Y;
    }

    @Test
    public void aDeadSlotIsNothingButAir() {
        // slot (0, 0) of the net carries no world
        int x = EDGE / 2;
        int z = EDGE / 2;
        mainLoop.forceAndWaitForGeneration(new Vector3i(x, CubeGeometry.DEFAULT_SEA_LEVEL, z));

        for (int y = 0; y < 64; y++) {
            assertEquals("engine:air", worldProvider.getBlock(x, y, z).getURI().toString(),
                    "a dead slot of the net is not empty at y=" + y);
        }
    }
}
