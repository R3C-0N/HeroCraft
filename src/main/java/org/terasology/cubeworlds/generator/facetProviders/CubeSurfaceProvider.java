// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facetProviders;

import org.joml.Vector2ic;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereSampler;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockAreac;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.ScalableFacetProvider;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generation.facets.SeaLevelFacet;

/**
 * The shape of the land, read off the sphere.
 * <p>
 * Two terms: a very large one that decides where the continents are, and a smaller one that gives
 * them relief, damped over the ocean so the sea floor stays quiet. Both are sampled through
 * {@link SphereSampler}, which is what makes them agree across a seam.
 * <p>
 * The amplitude is deliberately modest. The vertical extent of the relevance region is capped at a
 * handful of chunks against sixty-three horizontally, so a mountain range of a thousand blocks
 * would simply not be loaded.
 */
@Produces(ElevationFacet.class)
@Requires(@Facet(SeaLevelFacet.class))
public class CubeSurfaceProvider implements ScalableFacetProvider {

    /**
     * How many continents, and how many ranges of relief, fit around the sphere.
     * <p>
     * Counts and not sizes in blocks, for the reason {@link
     * org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider} sets out at length: a
     * size in blocks is a local feature on a large world and larger than the planet on a small one,
     * and the field then comes out uniform. These two were sizes until the world shrank from 8192
     * blocks to a face down to 3200; the counts here are the ones that reproduce what the larger
     * world had, so the shape of the land is unchanged by the move.
     */
    private static final double CONTINENT_PATCHES = 1.94;
    private static final double RELIEF_PATCHES = 12.0;

    /** How far above and below the reference level the land reaches. */
    private static final double CONTINENT_AMPLITUDE = 62.0;
    private static final double RELIEF_AMPLITUDE = 26.0;

    private final CubeGeometry geometry;
    private Noise continentNoise;
    private Noise reliefNoise;

    public CubeSurfaceProvider(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void setSeed(long seed) {
        // Each noise takes its own stream. Deriving them by adding a small constant to one seed
        // makes two fields that shift together when one is inserted; an independent offset per
        // field keeps a change local to that field.
        continentNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x5be7d1a1L), 5);
        reliefNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x1f0f3c77L), 7);
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        ElevationFacet facet = new ElevationFacet(region.getRegion(), region.getBorderForFacet(ElevationFacet.class));
        int seaLevel = region.getRegionFacet(SeaLevelFacet.class).getSeaLevel();

        BlockAreac area = facet.getWorldArea();
        int face = geometry.faceOfRegion(region.getRegion(), scale);
        if (face == CubePattern.NO_FACE) {
            // A dead slot of the net: nothing here, and nothing will be rasterized either.
            for (Vector2ic pos : area) {
                facet.setWorld(pos.x(), pos.y(), seaLevel - 100.0f);
            }
            region.setRegionFacet(ElevationFacet.class, facet);
            return;
        }

        // The sampler takes a size in blocks, so the counts above are turned into one here, against
        // this world's own radius.
        double continentSize = geometry.radius() / CONTINENT_PATCHES;
        double reliefSize = geometry.radius() / RELIEF_PATCHES;

        SphereSampler sampler = new SphereSampler(geometry, face);
        for (Vector2ic pos : area) {
            sampler.moveTo(pos.x() * scale, pos.y() * scale);
            double continent = sampler.read(continentNoise, continentSize);
            double land = Math.max(0.0, continent);
            double relief = sampler.read(reliefNoise, reliefSize);
            double elevation = seaLevel
                    + continent * CONTINENT_AMPLITUDE
                    + relief * RELIEF_AMPLITUDE * land;
            // In blocks at every scale, which is what every reader of the facet expects: the stock
            // density provider takes it as a height and the sunlight rasterizer compares it with
            // one. Divided by the scale, the ground of a coarse region landed at a fraction of its
            // height, under the sea everywhere.
            facet.setWorld(pos.x(), pos.y(), (float) elevation);
        }
        region.setRegionFacet(ElevationFacet.class, facet);
    }
}
