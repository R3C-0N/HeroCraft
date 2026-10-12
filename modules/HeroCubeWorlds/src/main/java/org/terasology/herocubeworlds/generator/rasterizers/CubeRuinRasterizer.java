// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.rasterizers;

import org.joml.Vector3i;
import org.terasology.herocubeworlds.generator.RuinCulture;
import org.terasology.herocubeworlds.generator.RuinShape;
import org.terasology.herocubeworlds.generator.facets.RuinFacet;
import org.terasology.engine.registry.CoreRegistry;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.WorldRasterizer;

import java.util.EnumMap;
import java.util.Map;

/**
 * Lays the ruins down, the part of each that falls in the chunk.
 * <p>
 * After the trees and before the sunlight: the court is cleared over whatever grew in it, and the
 * light is then computed over the ruin as it stands.
 * <p>
 * The foundation only fills air and liquid, so a ruin on a slope gets a plinth on the low side and
 * keeps the hill's own rock on the high side, where the clearing has cut it level.
 */
@Requires(@Facet(RuinFacet.class))
public class CubeRuinRasterizer implements WorldRasterizer {

    private final Map<RuinCulture, Map<RuinShape.Part, Block>> blocks = new EnumMap<>(RuinCulture.class);
    private Block air;

    @Override
    public void initialize() {
        BlockManager blockManager = CoreRegistry.get(BlockManager.class);
        air = blockManager.getBlock(BlockManager.AIR_ID);
        for (RuinCulture culture : RuinCulture.values()) {
            Map<RuinShape.Part, Block> parts = new EnumMap<>(RuinShape.Part.class);
            for (RuinShape.Part part : RuinShape.Part.values()) {
                if (part != RuinShape.Part.NONE && part != RuinShape.Part.CLEAR) {
                    parts.put(part, blockManager.getBlock(culture.blockOf(part)));
                }
            }
            blocks.put(culture, parts);
        }
    }

    @Override
    public void generateChunk(Chunk chunk, Region chunkRegion) {
        RuinFacet ruins = chunkRegion.getFacet(RuinFacet.class);
        if (ruins.getRuins().isEmpty()) {
            return;
        }
        int ox = chunk.getChunkWorldOffsetX();
        int oy = chunk.getChunkWorldOffsetY();
        int oz = chunk.getChunkWorldOffsetZ();
        Vector3i local = new Vector3i();
        for (RuinFacet.Ruin ruin : ruins.getRuins()) {
            Map<RuinShape.Part, Block> parts = blocks.get(ruin.culture());
            int cx = ruin.site().x();
            int cz = ruin.site().z();
            int floor = ruin.floorY();
            int minY = Math.max(oy, floor - RuinShape.FOUNDATION_DEPTH);
            int maxY = Math.min(oy + chunk.getChunkSizeY() - 1, floor + RuinShape.CLEAR_HEIGHT);
            int minX = Math.max(ox, cx - RuinShape.RADIUS);
            int maxX = Math.min(ox + chunk.getChunkSizeX() - 1, cx + RuinShape.RADIUS);
            int minZ = Math.max(oz, cz - RuinShape.RADIUS);
            int maxZ = Math.min(oz + chunk.getChunkSizeZ() - 1, cz + RuinShape.RADIUS);
            for (int y = minY; y <= maxY; y++) {
                for (int z = minZ; z <= maxZ; z++) {
                    for (int x = minX; x <= maxX; x++) {
                        RuinShape.Part part = RuinShape.part(x - cx, y - floor, z - cz, ruin.site().seed());
                        local.set(x - ox, y - oy, z - oz);
                        switch (part) {
                            case NONE:
                                break;
                            case CLEAR:
                                chunk.setBlock(local, air);
                                break;
                            case FOUNDATION:
                                Block there = chunk.getBlock(local);
                                if (there == air || there.isLiquid()) {
                                    chunk.setBlock(local, parts.get(part));
                                }
                                break;
                            default:
                                chunk.setBlock(local, parts.get(part));
                        }
                    }
                }
            }
        }
    }
}
