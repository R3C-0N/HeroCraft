// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.rasterizers;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.herocubeworlds.generator.OreKind;
import org.terasology.herocubeworlds.generator.facets.OreFacet;
import org.terasology.engine.registry.CoreRegistry;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.WorldRasterizer;

import java.util.EnumMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

/**
 * Turns the ore facet into blocks.
 * <p>
 * Nothing is decided here, and that is the point: the provider chose the cells without a block
 * manager anywhere near it, which is what lets {@code OreVolumeCensusTest} measure the whole design
 * with no engine running. This class only knows which block an {@link OreKind} is.
 * <p>
 * <b>It walks the entries, not the cells.</b> The facet is sparse and ore is a few parts per
 * thousand, so this touches a couple of hundred positions in a chunk instead of sixty-five thousand.
 * <p>
 * <b>And it replaces rock only.</b> The provider already keeps out of the soil, by the same
 * {@code CoreBiome.rockDepth} the solid rasterizer uses — but this is the one place that can see
 * what was actually laid down, so the rule is enforced where it can be checked rather than where it
 * has to be trusted. A cell a gallery took away holds no block at all, which is why ore can never
 * appear in mid-air in a tunnel whatever the chain does upstream.
 * <p>
 * Deliberately not a {@link org.terasology.engine.world.generation.ScalableWorldRasterizer}: a
 * level-of-detail cell stands for several blocks, so a six-block vein drawn out there would be a
 * scatter of specks in terrain nobody can reach. The provider skips those regions too.
 */
@Requires(@Facet(OreFacet.class))
public class CubeOreRasterizer implements WorldRasterizer {

    /**
     * What ore is allowed to sit in.
     * <p>
     * Stone and hard stone, plus the two rocks a region brings with it. Never soil, sand, peat, ash,
     * fulgurite, blight or ice: an ore in sand would come out of the ground with a shovel and have no
     * gangue to speak of, and an ore in a cap of ice would be a seam floating over the sea.
     */
    private static final String[] HOSTS = {
            "CoreAssets:Stone", "CoreAssets:HardStone", "CoreAssets:Basalt", "CoreAssets:Crystal"};

    private final Map<OreKind, Block> blocks = new EnumMap<>(OreKind.class);
    private final Set<Block> hosts = new HashSet<>();

    @Override
    public void initialize() {
        BlockManager blockManager = CoreRegistry.get(BlockManager.class);
        for (OreKind ore : OreKind.values()) {
            blocks.put(ore, blockManager.getBlock(ore.blockUri()));
        }
        for (String host : HOSTS) {
            hosts.add(blockManager.getBlock(host));
        }
    }

    @Override
    public void generateChunk(Chunk chunk, Region chunkRegion) {
        OreFacet ores = chunkRegion.getFacet(OreFacet.class);
        Vector3i local = new Vector3i();
        for (Map.Entry<Vector3ic, OreKind> entry : ores.getWorldEntries().entrySet()) {
            Vector3ic at = entry.getKey();
            local.set(at.x() - chunk.getChunkWorldOffsetX(),
                    at.y() - chunk.getChunkWorldOffsetY(),
                    at.z() - chunk.getChunkWorldOffsetZ());
            if (!chunk.getRegion().contains(at) || !hosts.contains(chunk.getBlock(local))) {
                continue;
            }
            chunk.setBlock(local, blocks.get(entry.getValue()));
        }
    }
}
