// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator;

import org.joml.Vector2i;
import org.joml.Vector2ic;
import org.joml.Vector3f;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.logic.spawner.AbstractSpawner;
import org.terasology.engine.math.SpiralIterable;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.facets.ElevationFacet;

/**
 * Finds dry land to spawn on, which on a cube world is not where one would put it.
 * <p>
 * The stock {@link org.terasology.engine.logic.spawner.FixedSpawner} was aimed at the middle of a
 * face and searched sixteen blocks around it. That is two orders of magnitude too small here: the
 * continent field has a wavelength of {@code radius / 1.94}, about a thousand blocks, so whether the
 * middle of a face is land or ocean is a coin toss and the nearest coast is hundreds of blocks away.
 * On the seed this was written against it came up ocean, and the player spawned underwater.
 * <p>
 * Widening the stock search was not an option. {@code AbstractSpawner.findSpawnPosition} asks for
 * {@link org.terasology.engine.world.generation.facets.SurfacesFacet} over a region grown by the
 * radius on <em>all three</em> axes, which drags in a dense 3D density facet of {@code (2r+1)^3}
 * floats: 36k cells at the stock radius of 16, a billion at 512. So this searches wide and cheaply
 * on its own — one 2D facet, nothing else — and hands the last sixteen blocks back to the stock
 * code, which already knows how to sit an entity on a surface.
 * <p>
 * Only the horizontal choice matters here. {@code PlayerFactory.findOpenVerticalPosition} recomputes
 * the height afterwards and counts any penetrable block as air — and water is penetrable, so it will
 * happily certify a pocket of ocean. Returning a better Y would change nothing; returning a better
 * X and Z is the whole fix.
 */
public class CubeLandSpawner extends AbstractSpawner {

    private static final Logger logger = LoggerFactory.getLogger(CubeLandSpawner.class);

    /**
     * How far apart the probes are, in blocks. Four chunks.
     * <p>
     * A probe costs one facet region whatever its spacing, so the spacing decides how much world a
     * fixed budget covers. Relief has a wavelength of {@code radius / 12}, about 164 blocks, and a
     * continent about a thousand: a 128-block lattice cannot step over a landmass. Probing every
     * chunk instead would cover sixteen times less ground for the same price and find nothing at all
     * in an ocean this size.
     */
    private static final int PROBE_STEP = 4 * Chunks.SIZE_X;

    /** How far above the reference level a column has to stand to count as dry. */
    private static final int DRY_MARGIN = 3;

    /** Handed to the stock search once a landing zone is known. Its own cost is cubic, so it stays small. */
    private static final int FINE_RADIUS = 16;

    private final CubeGeometry geometry;
    private final int face;

    private World searched;
    private Vector3f found;
    private int probes;

    public CubeLandSpawner(CubeGeometry geometry, int face) {
        this.geometry = geometry;
        this.face = face;
    }

    @Override
    public Vector3f getSpawnPosition(World world, EntityRef entity) {
        // Keyed on the world instance rather than cached outright: setWorldSeed drops the world and
        // builds a new one, and a spawn point from the previous seed would be somewhere else
        // entirely. PlayerSystem asks again on every death, and a fresh search each time would be a
        // visible stall.
        if (found == null || searched != world) {
            found = search(world);
            searched = world;
        }
        return new Vector3f(found);
    }

    /** How many probes the last search took. For the test that holds it to a budget. */
    public int probeCount() {
        return probes;
    }

    private Vector3f search(World world) {
        int[] centre = geometry.faceCentre(face);
        int seaLevel = geometry.seaLevel();
        int maxRadius = geometry.faceBlocks() / (2 * PROBE_STEP);

        Vector2i best = null;
        float bestHeight = Float.NEGATIVE_INFINITY;
        probes = 0;

        SpiralIterable spiral = SpiralIterable.clockwise(new Vector2i(centre[0], centre[1]))
                .maxRadius(maxRadius).scale(PROBE_STEP).build();
        for (Vector2ic probe : spiral) {
            if (geometry.faceOf(probe.x(), probe.y()) != face) {
                // Off the face, or on a dead slot of the net. Not an error: the spiral is square and
                // the face is square, so the corners of the last rings fall outside.
                continue;
            }
            probes++;
            ElevationFacet elevation = elevationAround(world, probe);
            for (Vector2ic column : elevation.getWorldArea()) {
                // The facet carries a border, which reaches past the chunk and can land on the face
                // next door or on a dead slot, where the elevation is a sentinel a hundred blocks
                // under the sea. Asking for the face rather than merely for "not a dead slot" keeps
                // the answer on the face the spiral is exploring.
                if (geometry.faceOf(column.x(), column.y()) != face) {
                    continue;
                }
                float height = elevation.getWorld(column);
                if (height > bestHeight) {
                    bestHeight = height;
                    best = new Vector2i(column);
                }
            }
            if (bestHeight >= seaLevel + DRY_MARGIN) {
                logger.info("cube spawn: land at {} {} ({} blocks up) after {} probes",
                        best.x, best.y, bestHeight, probes);
                return findSpawnPosition(world, best, FINE_RADIUS);
            }
        }

        if (best == null) {
            // The spiral starts on the centre of the face, so at least one probe must have landed.
            // If none did, the geometry and the net disagree and every later answer would be
            // nonsense; better to say which invariant broke than to spawn somewhere arbitrary.
            throw new IllegalStateException("no column of face " + face + " was reachable from its "
                    + "own centre: the spiral and the net disagree");
        }

        // Nothing dry on the whole face. Say so, loudly, and take the highest column seen — the
        // stock code reaches the same place by falling through a second loop without a word, and
        // that silence is what made this bug take a full play session to notice.
        logger.warn("cube spawn: no dry land anywhere on face {} after {} probes; falling back to "
                + "the highest column found, {} {} at {}", face, probes, best.x, best.y, bestHeight);
        return findSpawnPosition(world, best, FINE_RADIUS);
    }

    /** One chunk's worth of columns around a probe: the border is amortised over 1024 of them. */
    private ElevationFacet elevationAround(World world, Vector2ic probe) {
        BlockRegion area = new BlockRegion(probe.x(), geometry.seaLevel(), probe.y())
                .setSize(Chunks.SIZE_X, 1, Chunks.SIZE_Z);
        Region region = world.getWorldData(area);
        return region.getFacet(ElevationFacet.class);
    }
}
