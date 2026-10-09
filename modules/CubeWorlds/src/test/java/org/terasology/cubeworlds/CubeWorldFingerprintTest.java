// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * The cube world, block for block, is the one it was.
 * <p>
 * The chunks are where the world is hardest to get right: the middle of a face at the surface, in
 * the galleries and on the mantle; the edge of a cut seam; a corner, where three faces meet; the
 * polar face; and a dead slot, which must stay empty. The seed is the testing environment's own.
 * <p>
 * <b>Updating the number is a decision, not a fix.</b> It changes when the world changes, and a
 * change that was meant to leave the world alone and moves it has just been caught.
 */
@IntegrationEnvironment(worldGenerator = "CubeWorlds:cubeworld")
public class CubeWorldFingerprintTest {

    /** Recorded on CubeWorlds 53179c2, before the distant terrain touched the generator. */
    static final long EXPECTED = 0xa8ced7d308e4cb41L;

    @In
    private WorldProvider worldProvider;

    @In
    private MainLoop mainLoop;

    @Test
    public void theWorldIsTheOneItWas() {
        long fingerprint = WorldFingerprint.of(worldProvider, mainLoop, "cube",
                new Vector3i(150, 0, 150),
                new Vector3i(150, -1, 150),
                new Vector3i(150, -3, 150),
                new Vector3i(100, 0, 250),
                new Vector3i(100, 0, 199),
                new Vector3i(150, 0, 250),
                new Vector3i(50, 0, 50));
        assertEquals(String.format("%016x", EXPECTED), String.format("%016x", fingerprint),
                "the cube world is not the one it was");
    }
}
