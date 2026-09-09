// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.rasterizers;

import org.joml.Vector2i;
import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.biomesAPI.Biome;
import org.terasology.biomesAPI.BiomeRegistry;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.registry.CoreRegistry;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.ScalableWorldRasterizer;
import org.terasology.engine.world.generation.facets.DensityFacet;
import org.terasology.engine.world.generation.facets.SeaLevelFacet;
import org.terasology.engine.world.generation.facets.SurfacesFacet;

/**
 * Lays down the ground, and leaves the dead slots of the net empty.
 * <p>
 * The stock rasterizer would fill them with ocean up to the reference level, because to it a column
 * with no land is a column of water. Ten of the sixteen slots carry no world at all, so they have
 * to be told apart from a sea floor, and this is the only place that knows the difference.
 */
@Requires({
        @Facet(DensityFacet.class),
        @Facet(SurfacesFacet.class),
        @Facet(BiomeFacet.class),
        @Facet(SeaLevelFacet.class)})
public class CubeSolidRasterizer implements ScalableWorldRasterizer {

    private final CubeGeometry geometry;
    private Block water;
    private Block ice;
    private BiomeRegistry biomeRegistry;

    public CubeSolidRasterizer(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void initialize() {
        BlockManager blockManager = CoreRegistry.get(BlockManager.class);
        biomeRegistry = CoreRegistry.get(BiomeRegistry.class);
        water = blockManager.getBlock("CoreAssets:Water");
        ice = blockManager.getBlock("CoreAssets:Ice");
    }

    @Override
    public void generateChunk(Chunk chunk, Region chunkRegion, float scale) {
        if (geometry.faceOfRegion(chunkRegion.getRegion(), scale) == CubePattern.NO_FACE) {
            // Off the net: air, and not a drop of water.
            return;
        }

        DensityFacet solidityFacet = chunkRegion.getFacet(DensityFacet.class);
        SurfacesFacet surfacesFacet = chunkRegion.getFacet(SurfacesFacet.class);
        BiomeFacet biomeFacet = chunkRegion.getFacet(BiomeFacet.class);
        int seaLevel = chunkRegion.getFacet(SeaLevelFacet.class).getSeaLevel();

        Vector2i pos2d = new Vector2i();
        Vector3i worldPos = new Vector3i();
        for (Vector3ic pos : Chunks.CHUNK_REGION) {
            pos2d.set(pos.x(), pos.z());
            float density = solidityFacet.get(pos);
            float basePosY = (pos.y() + chunk.getChunkWorldOffsetY()) * scale;
            float posY = basePosY + Math.max(0, Math.min(scale, density));
            chunk.chunkToWorldPosition(pos, worldPos);

            Biome biome = biomeFacet.get(pos2d);
            biomeRegistry.setBiome(biome, chunk, pos.x(), pos.y(), pos.z());

            if (posY < seaLevel && basePosY + scale > seaLevel && seaLevel < scale) {
                chunk.setBlock(pos, water);
            } else if (density > 0 && surfacesFacet.get(pos)) {
                chunk.setBlock(pos, biome.getSurfaceBlock(worldPos, seaLevel));
            } else if (density > 0) {
                chunk.setBlock(pos, biome.getBelowSurfaceBlock(worldPos, density));
            } else if (posY <= seaLevel) {
                if (posY + scale > seaLevel && CoreBiome.SNOW == biome) {
                    chunk.setBlock(pos, ice);
                } else {
                    chunk.setBlock(pos, water);
                }
            }
        }
    }
}
