// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;

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
@IntegrationEnvironment(worldGenerator = "CubeWorlds:cubeworld")
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
