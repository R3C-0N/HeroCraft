// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * The flat world is the oracle of non-regression, so it is fingerprinted like the cube.
 * <p>
 * Nothing the cube does is supposed to reach it: it shares the stock providers of CoreWorlds, and
 * any change made there for the cube's sake — a material table, a unit of height — shows up here
 * first.
 */
@IntegrationEnvironment(worldGenerator = "CoreWorlds:facetedsimplex")
public class FlatWorldFingerprintTest {

    /** Recorded on CoreWorlds e43e41a, before the distant terrain touched the generator. */
    static final long EXPECTED = 0x1905ab84bf55a6afL;

    @In
    private WorldProvider worldProvider;

    @In
    private MainLoop mainLoop;

    @Test
    public void theWorldIsTheOneItWas() {
        long fingerprint = WorldFingerprint.of(worldProvider, mainLoop, "plat",
                new Vector3i(0, 0, 0),
                new Vector3i(0, -1, 0),
                new Vector3i(7, 0, -5));
        assertEquals(String.format("%016x", EXPECTED), String.format("%016x", fingerprint),
                "the flat world is not the one it was");
    }
}
