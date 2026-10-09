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
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.entity.placement.PlaceBlocks;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * The build limit, exercised through the event the game really sends.
 * <p>
 * Every test here comes in a pair, and the pair is the point: a refusal on its own proves nothing,
 * because a placement that never worked in the first place also leaves the block unplaced. The
 * control places a block one step inside the bound and asserts it lands.
 */
@IntegrationEnvironment(worldGenerator = "HeroCubeWorlds:cubeworld")
public class VerticalBuildLimitTest {

    @In
    private WorldProvider worldProvider;

    @In
    private MainLoop mainLoop;

    @In
    private BlockManager blockManager;

    private static final int EDGE = CubeGeometry.DEFAULT_FACE_CHUNKS * 32;

    /** The middle of face 1, where the generator puts the spawn. */
    private static final int X = (int) (1.5 * EDGE);
    private static final int Z = (int) (1.5 * EDGE);

    @Test
    public void nothingIsBuiltAboveTheCeiling() {
        Vector3i outside = new Vector3i(X, CubeGeometry.DEFAULT_CEILING_Y + 1, Z);
        Vector3i inside = new Vector3i(X, CubeGeometry.DEFAULT_CEILING_Y, Z);
        place(outside);
        place(inside);

        assertEquals("engine:air", uriAt(outside), "a block was built above the top of the world");
        assertEquals("coreassets:stone", uriAt(inside),
                "the control failed: nothing can be built at the ceiling either, so the refusal "
                        + "above it proves nothing");
    }

    @Test
    public void nothingIsBuiltBelowTheFloor() {
        Vector3i outside = new Vector3i(X, CubeGeometry.DEFAULT_FLOOR_Y - 1, Z);
        place(outside);

        assertEquals("engine:air", uriAt(outside), "a block was built below the bottom of the world");
    }

    private void place(Vector3i position) {
        mainLoop.forceAndWaitForGeneration(position);
        Block stone = blockManager.getBlock("CoreAssets:Stone");
        worldProvider.getWorldEntity().send(new PlaceBlocks(new Vector3i(position), stone));
    }

    /**
     * Lower-cased, because a block URI does not print with the case it was asked for: the stone
     * family comes back as {@code coreassets:stone} however it is spelled going in. Comparing the
     * printed form as written is how this test first failed, on a block that had been placed
     * perfectly well.
     */
    private String uriAt(Vector3i position) {
        return worldProvider.getBlock(position).getURI().toString().toLowerCase(java.util.Locale.ROOT);
    }
}
