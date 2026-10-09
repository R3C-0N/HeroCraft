// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facetProviders;

import org.joml.Vector2ic;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.SphereSampler;
import org.terasology.herocubeworlds.generator.facets.ArcaneMaskFacet;
import org.terasology.herocubeworlds.generator.facets.LatitudeFacet;
import org.terasology.herocubeworlds.generator.facets.MiasmaMaskFacet;
import org.terasology.herocubeworlds.generator.facets.VolcanicMaskFacet;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockAreac;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.ScalableFacetProvider;

/**
 * Where the extreme regions are, and which hemisphere a column is in.
 * <p>
 * Three of the seven regions — the seas of lava, the unstable magic, the miasmic swamps — are not
 * decided by the climate. They cannot be: a region carved out of temperature, humidity and height
 * alone comes out in crumbs, because those fields already spent themselves telling forest from
 * plain. Each one gets a field of its own instead, read on the sphere the way the climate is, so
 * that a patch does not stop at the edge of a face.
 * <p>
 * The fields are raised to a power rather than thresholded here. A mask that is already zero or one
 * by the time the biome sees it would give the region a hard edge, and a hard edge is what a region
 * opened by a whole piece of technology should least look like. The softening belongs here, at the
 * source, and not in the cascade that reads it.
 * <p>
 * This is also the one place in the chain that touches the sphere on behalf of the biomes, which is
 * why the signed latitude comes out of it as a facet: everything downstream then decides on facets
 * alone, and can be tested without a geometry.
 */
@Produces({VolcanicMaskFacet.class, ArcaneMaskFacet.class, MiasmaMaskFacet.class, LatitudeFacet.class})
public class CubeRegionProvider implements ScalableFacetProvider {

    /**
     * How many patches of each region fit around the sphere.
     * <p>
     * A count and not a size in blocks, and that distinction is the whole reason these fields work.
     * The default world is 3200 blocks to a face and the world the tests measure is 512, so a patch
     * of "1800 blocks" is a local feature in one and larger than the planet in the other — which is
     * exactly what happened: on the test world the arcane and miasmic fields came out very nearly
     * uniform, both regions landed in the sea, and both existed nowhere. Counted around the sphere,
     * a patch is the same share of the world at any size.
     * <p>
     * The lesson took a second time to land: the surface and climate fields were still written in
     * blocks when the face went from 8192 down to 3200, and at that size one of them was larger than
     * a quarter of the circumference. They are counts now too.
     * <p>
     * The seas of lava sit in the mountain ranges, so they are the coarsest of the three; the miasma
     * is the finest, because a swamp is a local thing.
     */
    private static final double VOLCANIC_PATCHES = 7.0;
    private static final double ARCANE_PATCHES = 9.0;
    private static final double MIASMA_PATCHES = 12.0;

    /**
     * How sharply each field falls away from its peaks.
     * <p>
     * Raising a field in [0, 1] to a power leaves its maximum where it was and pulls everything else
     * down, which is how these regions end up rare without a threshold having to be moved. The
     * exponents are what make the extremes a few percent of the world rather than a third of it.
     */
    private static final double VOLCANIC_FALLOFF = 6.0;
    private static final double ARCANE_FALLOFF = 6.0;
    private static final double MIASMA_FALLOFF = 5.0;

    private final CubeGeometry geometry;
    private Noise volcanicNoise;
    private Noise arcaneNoise;
    private Noise miasmaNoise;

    public CubeRegionProvider(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void setSeed(long seed) {
        volcanicNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x5f1a03c7L), 4);
        arcaneNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x1d7be92fL), 4);
        miasmaNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x64c8a115L), 5);
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        VolcanicMaskFacet volcanic = new VolcanicMaskFacet(
                region.getRegion(), region.getBorderForFacet(VolcanicMaskFacet.class));
        ArcaneMaskFacet arcane = new ArcaneMaskFacet(
                region.getRegion(), region.getBorderForFacet(ArcaneMaskFacet.class));
        MiasmaMaskFacet miasma = new MiasmaMaskFacet(
                region.getRegion(), region.getBorderForFacet(MiasmaMaskFacet.class));
        LatitudeFacet latitude = new LatitudeFacet(
                region.getRegion(), region.getBorderForFacet(LatitudeFacet.class));

        int face = geometry.faceOfRegion(region.getRegion(), scale);
        BlockAreac area = volcanic.getWorldArea();
        if (face == CubePattern.NO_FACE) {
            // Off the net there is no world, so there is no region and no latitude either. Zero
            // rather than a neutral value: a mask is an amount, and here the amount is none.
            for (Vector2ic pos : area) {
                volcanic.setWorld(pos.x(), pos.y(), 0f);
                arcane.setWorld(pos.x(), pos.y(), 0f);
                miasma.setWorld(pos.x(), pos.y(), 0f);
                latitude.setWorld(pos.x(), pos.y(), 0f);
            }
            publish(region, volcanic, arcane, miasma, latitude);
            return;
        }

        // The sampler takes a size in blocks, so the counts above are turned into one here, against
        // this world's own radius.
        double volcanicSize = geometry.radius() / VOLCANIC_PATCHES;
        double arcaneSize = geometry.radius() / ARCANE_PATCHES;
        double miasmaSize = geometry.radius() / MIASMA_PATCHES;

        SphereSampler sampler = new SphereSampler(geometry, face);
        for (Vector2ic pos : area) {
            sampler.moveTo(pos.x() * scale, pos.y() * scale);
            volcanic.setWorld(pos.x(), pos.y(), mask(sampler, volcanicNoise, volcanicSize, VOLCANIC_FALLOFF));
            arcane.setWorld(pos.x(), pos.y(), mask(sampler, arcaneNoise, arcaneSize, ARCANE_FALLOFF));
            miasma.setWorld(pos.x(), pos.y(), mask(sampler, miasmaNoise, miasmaSize, MIASMA_FALLOFF));
            latitude.setWorld(pos.x(), pos.y(), (float) sampler.sinLatitude());
        }

        publish(region, volcanic, arcane, miasma, latitude);
    }

    private static void publish(GeneratingRegion region, VolcanicMaskFacet volcanic, ArcaneMaskFacet arcane,
                                MiasmaMaskFacet miasma, LatitudeFacet latitude) {
        region.setRegionFacet(VolcanicMaskFacet.class, volcanic);
        region.setRegionFacet(ArcaneMaskFacet.class, arcane);
        region.setRegionFacet(MiasmaMaskFacet.class, miasma);
        region.setRegionFacet(LatitudeFacet.class, latitude);
    }

    /** A field in [0, 1], peaked where the noise is highest and soft everywhere else. */
    private static float mask(SphereSampler sampler, Noise noise, double featureSize, double falloff) {
        double raw = (sampler.read(noise, featureSize) + 1.0) * 0.5;
        return (float) Math.pow(Math.max(0.0, Math.min(1.0, raw)), falloff);
    }
}
