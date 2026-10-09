// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector2fc;
import org.joml.Vector3i;
import org.terasology.biomesAPI.Biome;
import org.terasology.cubeworlds.generator.rasterizers.CubeCell;
import org.terasology.cubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.engine.rendering.assets.texture.TextureData;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockColorSource;
import org.terasology.engine.world.block.BlockPart;
import org.terasology.engine.world.block.DefaultColorSource;
import org.terasology.engine.world.block.shapes.BlockMeshPart;
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
 * <p>
 * The tint of a chunk's vertex is the block's colour offset times the colour of its colour source,
 * and grass and leaves take theirs from the biome: {@code BiomeColorProvider} reads it off a table,
 * grass or foliage, at the biome's temperature and humidity. Without it the distance drew grass in
 * the bare colour of its tile and leaves in grey, when the terrain drew them in the green of their
 * biome. So a block whose colour comes from a table is numbered once per biome it is met in, and
 * each number carries its colour; every other block has one number whatever the biome.
 */
public final class DistantPalette implements ColumnSummarizer.MaterialNamer, DistantMesher.Looks {

    private final CubeSolidRasterizer rasterizer;
    private final Map<Block, Integer> ids = new ConcurrentHashMap<>();
    private final Map<Biome, Map<Integer, Integer>> tintedIds = new ConcurrentHashMap<>();
    private final List<Block> blocks = new CopyOnWriteArrayList<>();
    private final List<Colorc[]> tints = new CopyOnWriteArrayList<>();
    private final List<Integer> bases = new CopyOnWriteArrayList<>();
    private volatile TextureData grassColours;
    private volatile TextureData foliageColours;

    /** @param rasterizer an initialised rasterizer: its blocks are resolved */
    public DistantPalette(CubeSolidRasterizer rasterizer) {
        this.rasterizer = rasterizer;
    }

    /**
     * The tables the biome colours are read from, when the world has a colour provider; without one
     * a chunk tints nothing, and neither does the distance. Before any block is numbered.
     */
    public void setBiomeColours(TextureData grass, TextureData foliage) {
        this.grassColours = grass;
        this.foliageColours = foliage;
    }

    @Override
    public int id(CubeCell cell, Biome biome, Vector3i worldPos, int seaLevel, float density) {
        Block block = rasterizer.blockFor(cell, biome, worldPos, seaLevel, density);
        return block == null ? -1 : tinted(idOf(block), biome);
    }

    /** The number of a block, given it the first time it is met, whatever the biome. Any thread. */
    public int idOf(Block block) {
        Integer id = ids.get(block);
        if (id != null) {
            return id;
        }
        synchronized (blocks) {
            return ids.computeIfAbsent(block, b -> register(b, null, -1));
        }
    }

    /** The number of this material as it looks in this biome: itself, unless its colour comes from the biome. */
    @Override
    public int tinted(int material, Biome biome) {
        if (material < 0 || biome == null || grassColours == null) {
            return material;
        }
        int base = bases.get(material);
        Block block = blocks.get(base);
        if (!takesBiomeColour(block)) {
            return base;
        }
        Map<Integer, Integer> forBiome = tintedIds.computeIfAbsent(biome, b -> new ConcurrentHashMap<>());
        Integer id = forBiome.get(base);
        if (id != null) {
            return id;
        }
        synchronized (blocks) {
            return forBiome.computeIfAbsent(base, b -> register(block, biome, base));
        }
    }

    /** Under the lock. */
    private int register(Block block, Biome biome, int base) {
        int id = blocks.size();
        blocks.add(block);
        tints.add(new Colorc[]{colour(block, biome, BlockPart.TOP), colour(block, biome, BlockPart.FRONT)});
        bases.add(base < 0 ? id : base);
        return id;
    }

    private static boolean takesBiomeColour(Block block) {
        for (BlockPart part : BlockPart.values()) {
            BlockColorSource source = block.getColorSource(part);
            if (source == DefaultColorSource.COLOR_LUT || source == DefaultColorSource.FOLIAGE_LUT) {
                return true;
            }
        }
        return false;
    }

    /** What a chunk would give a face of this block in this biome: the colour source times the offset. */
    private Colorc colour(Block block, Biome biome, BlockPart part) {
        Colorc offset = block.getColorOffset(part);
        BlockColorSource source = block.getColorSource(part);
        TextureData table = source == DefaultColorSource.COLOR_LUT ? grassColours
                : source == DefaultColorSource.FOLIAGE_LUT ? foliageColours : null;
        if (table == null || biome == null) {
            return offset;
        }
        // BiomeColorProvider's lookup, to the pixel: across by temperature, down by their product.
        float temperature = biome.getTemperature();
        float humidity = biome.getHumidity() * temperature;
        Colorc lut = table.getPixel((int) ((1 - temperature) * 255), (int) ((1 - humidity) * 255), new Color());
        return new Color(lut.rf() * offset.rf(), lut.gf() * offset.gf(), lut.bf() * offset.bf(),
                lut.af() * offset.af());
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
        return tints.get(material)[0];
    }

    @Override
    public Colorc sideTint(int material) {
        return tints.get(material)[1];
    }

    @Override
    public boolean isGrass(int material) {
        return block(material).isGrass();
    }

    @Override
    public boolean isWater(int material) {
        return block(material).isWater();
    }

    @Override
    public int frames(int material) {
        BlockMeshPart top = block(material).getPrimaryAppearance().getPart(BlockPart.TOP);
        return top == null ? 1 : Math.max(1, top.getTexFrames());
    }
}
