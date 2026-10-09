// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3f;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.CubeLandSpawner;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.ElevationFacet;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * That the player is put on dry land, and that finding it does not cost the world.
 * <p>
 * The witness is the whole test. A spawner that searched nothing and returned the middle of the face
 * would satisfy "is on face 1" and "is somewhere"; only measuring the middle of the face separately,
 * and showing it is underwater, proves the search moved. That is the shape this module's tests
 * already use, and it is the shape that would have caught this bug before a play session did.
 */
class CubeSpawnTest {

    /** Matches the private constant in the spawner; a spawn at the waterline is not dry land. */
    private static final int DRY_MARGIN = 3;

    @Test
    void theSpawnIsOnDryLandAndTheMiddleOfTheFaceIsNot() {
        CubeGeometry cube = CubeGeometry.createDefault();
        World world = build(cube);

        int[] centre = cube.faceCentre(1);
        float atCentre = elevationAt(world, centre[0], centre[1]);

        CubeLandSpawner spawner = new CubeLandSpawner(cube, 1);
        Vector3f spawn = spawner.getSpawnPosition(world, EntityRef.NULL);
        float atSpawn = elevationAt(world, (int) spawn.x, (int) spawn.z);

        System.out.printf("middle of face 1 (%d, %d): elevation %.1f, sea level %d%n",
                centre[0], centre[1], atCentre, cube.seaLevel());
        System.out.printf("spawn (%.0f, %.0f): elevation %.1f, found in %d probes%n",
                spawn.x, spawn.z, atSpawn, spawner.probeCount());
        System.out.printf("the search moved %.0f blocks%n",
                Math.hypot(spawn.x - centre[0], spawn.z - centre[1]));

        // The witness. If this ever fails, the test has stopped proving anything: it would mean the
        // middle of the face is already dry on this seed, so a spawner that searched nothing would
        // pass every other assertion below.
        assertTrue(atCentre < cube.seaLevel() + DRY_MARGIN,
                "the middle of face 1 is already dry land on this seed (" + atCentre + "), so this "
                        + "test can no longer tell a search from a constant — pick another seed");

        assertEquals(1, cube.faceOf((int) spawn.x, (int) spawn.z),
                "the spawn wandered off face 1, onto another face or a dead slot of the net");
        assertTrue(atSpawn >= cube.seaLevel(),
                "the spawn is under water: elevation " + atSpawn + " against a sea level of "
                        + cube.seaLevel());
    }

    /**
     * The budget. A probe is one 2D facet region, and the spiral is bounded by the face, so the
     * worst case is a face with no land at all — which must still not turn world creation into a
     * hang. This is a bound on loading time, not a micro-optimisation.
     */
    @Test
    void theSearchIsBounded() {
        CubeGeometry cube = CubeGeometry.createDefault();
        World world = build(cube);
        CubeLandSpawner spawner = new CubeLandSpawner(cube, 1);

        long start = System.nanoTime();
        spawner.getSpawnPosition(world, EntityRef.NULL);
        double ms = (System.nanoTime() - start) / 1e6;

        System.out.printf("the spawn search took %d probes and %.0f ms%n", spawner.probeCount(), ms);
        assertTrue(spawner.probeCount() <= 625,
                "the search took " + spawner.probeCount() + " probes; the spiral is meant to be "
                        + "bounded by the face, so this means it is escaping it");
        assertTrue(ms < 4000, "the spawn search took " + ms + " ms, which a player creating a world "
                + "would feel as a hang");
    }

    /** The second call must not search again: PlayerSystem asks on every death. */
    @Test
    void theAnswerIsRemembered() {
        CubeGeometry cube = CubeGeometry.createDefault();
        World world = build(cube);
        CubeLandSpawner spawner = new CubeLandSpawner(cube, 1);

        Vector3f first = spawner.getSpawnPosition(world, EntityRef.NULL);
        int afterFirst = spawner.probeCount();
        Vector3f second = spawner.getSpawnPosition(world, EntityRef.NULL);

        assertEquals(first, second, "two calls gave two different spawn points");
        assertEquals(afterFirst, spawner.probeCount(),
                "the second call searched again; respawning would stall for as long as world "
                        + "creation did");
    }

    private static float elevationAt(World world, int x, int z) {
        return world.getWorldData(new BlockRegion(x, 0, z))
                .getFacet(ElevationFacet.class).getWorld(x, z);
    }

    /**
     * Only what the elevation needs. The spawn search reads {@link ElevationFacet} and nothing else
     * — that is the point of it, since asking for surfaces drags in a dense 3D density facet — so a
     * fuller chain would slow the test without testing anything more.
     */
    private static World build(CubeGeometry cube) {
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(cube.seaLevel())
                .addProvider(new SeaLevelProvider(cube.seaLevel()))
                .addProvider(new CubeSurfaceProvider(cube));
        builder.setSeed(0xC0FFEE);
        return builder.build();
    }
}
