// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.viewer.layers;

import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facets.LatitudeFacet;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.viewer.color.ColorBlender;
import org.terasology.engine.world.viewer.color.ColorBlenders;
import org.terasology.engine.world.viewer.color.ColorModels;
import org.terasology.engine.world.viewer.layers.AbstractFacetLayer;
import org.terasology.engine.world.viewer.layers.Renders;
import org.terasology.nui.Color;

import java.awt.image.BufferedImage;
import java.awt.image.DataBufferInt;

/**
 * Paints the ten dead slots of the net black, so the preview stops calling them ocean.
 * <p>
 * Nothing in the stock chain can tell the two apart, and not for want of trying: a dead slot has no
 * world, so {@code CubeSurfaceProvider} marks it with an elevation of a hundred blocks below the
 * sea. The biome provider reads that as {@code ABYSS} and paints it very nearly black — and then
 * three grey field layers add themselves on top, temperature and humidity among them, which sit at
 * a flat 0.5 over a dead slot and are drawn at a scale of a hundred. Two hundred added to a near
 * black is a plausible flat blue-grey. The map looked like open sea because every layer in the
 * chain was answering honestly.
 * <p>
 * This one writes opaque, at an order above them all, so it is the last word rather than another
 * contribution. It hangs off {@link LatitudeFacet} only because a layer must name a facet and that
 * one has no other renderer; what it draws depends on the region's coordinates, not on the facet.
 */
@Renders(value = LatitudeFacet.class, order = DeadSlotFacetLayer.ORDER)
public class DeadSlotFacetLayer extends AbstractFacetLayer {

    /**
     * Above {@code ZOrder.TREES} (3000), which is the highest the stock layers use. A layer that is
     * merely opaque is not enough: the three grey ones at {@code SURFACE} would still be added on
     * top of it.
     */
    public static final int ORDER = 4000;

    private static final Color VOID = new Color(0x000000ff);

    private final CubeGeometry geometry = CubeGeometry.createDefault();

    @Override
    public void render(BufferedImage img, Region region) {
        BlockRegionc bounds = region.getRegion();
        int width = img.getWidth();
        int height = img.getHeight();

        ColorBlender blender = ColorBlenders.forColorModel(ColorModels.RGBA, img.getColorModel());
        DataBufferInt dataBuffer = (DataBufferInt) img.getRaster().getDataBuffer();
        int black = blender.get(VOID.rgba());

        for (int z = 0; z < height; z++) {
            for (int x = 0; x < width; x++) {
                if (!CubePattern.isAlive(bounds.minX() + x, bounds.minZ() + z, geometry.faceBlocks())) {
                    dataBuffer.setElem(z * width + x, black);
                }
            }
        }
    }

    @Override
    public String getWorldText(Region region, int wx, int wy) {
        int face = geometry.faceOf(wx, wy);
        return face == CubePattern.NO_FACE ? "hors du patron" : CubePattern.NAMES[face];
    }
}
