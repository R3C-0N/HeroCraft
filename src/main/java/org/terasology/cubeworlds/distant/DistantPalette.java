// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector2fc;
import org.joml.Vector3i;
import org.terasology.biomesAPI.Biome;
import org.terasology.cubeworlds.generator.rasterizers.CubeCell;
import org.terasology.cubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockPart;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * The materials of the distant terrain, named by block — the very block the rasterizer would write.
 * <p>
 * A cell's block comes from {@link CubeSolidRasterizer#blockFor}, never from a table kept here, so a
 * change of material in the world is a change in the distance too. A block is numbered the first
 * time it is met, from any of the building threads; what it looks like is read off its appearance:
 * the tile of its top and of its side in the atlas, and their tint.
 */
public final class DistantPalette implements ColumnSummarizer.MaterialNamer, DistantMesher.Looks {

    /**
     * The sea seen from afar, as deep water looks: the refractive pass that gives the real water its
     * depth does not run on the distance, where the surface is drawn opaque. A tint, not a new
     * material — the approximation is named, not hidden.
     */
    private static final Colorc DEEP_WATER = new Color(0.30f, 0.45f, 0.62f, 1.0f);

    private final CubeSolidRasterizer rasterizer;
    private final Map<Block, Integer> ids = new ConcurrentHashMap<>();
    private final List<Block> blocks = new CopyOnWriteArrayList<>();

    /** @param rasterizer an initialised rasterizer: its blocks are resolved */
    public DistantPalette(CubeSolidRasterizer rasterizer) {
        this.rasterizer = rasterizer;
    }

    @Override
    public int id(CubeCell cell, Biome biome, Vector3i worldPos, int seaLevel, float density) {
        Block block = rasterizer.blockFor(cell, biome, worldPos, seaLevel, density);
        return block == null ? -1 : idOf(block);
    }

    /** The number of a block, given it the first time it is met. Any thread. */
    public int idOf(Block block) {
        Integer id = ids.get(block);
        if (id != null) {
            return id;
        }
        synchronized (blocks) {
            return ids.computeIfAbsent(block, b -> {
                blocks.add(b);
                return blocks.size() - 1;
            });
        }
    }

    public Block block(int material) {
        return blocks.get(material);
    }

    @Override
    public boolean isLiquid(int material) {
        return block(material).isLiquid();
    }

    @Override
    public Vector2fc topTile(int material) {
        return block(material).getPrimaryAppearance().getTextureAtlasPos(BlockPart.TOP);
    }

    @Override
    public Vector2fc sideTile(int material) {
        return block(material).getPrimaryAppearance().getTextureAtlasPos(BlockPart.FRONT);
    }

    @Override
    public Colorc topTint(int material) {
        Block block = block(material);
        if (block.isLiquid() && block.getLuminance() == 0) {
            return DEEP_WATER;
        }
        return block.getColorOffset(BlockPart.TOP);
    }

    @Override
    public Colorc sideTint(int material) {
        return block(material).getColorOffset(BlockPart.FRONT);
    }

    @Override
    public boolean isGrass(int material) {
        return block(material).isGrass();
    }
}
