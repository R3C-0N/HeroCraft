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
import org.terasology.cubeworlds.generator.facets.CaveFacet;
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
 * Lays down the ground, ends the world top and bottom, and leaves the dead slots of the net empty.
 * <p>
 * The stock rasterizer would fill them with ocean up to the reference level, because to it a column
 * with no land is a column of water. Ten of the sixteen slots carry no world at all, so they have
 * to be told apart from a sea floor, and this is the only place that knows the difference.
 * <p>
 * It is also the only place that can end the world vertically. The density is {@code elevation - y}
 * and nothing bounds it, so left alone the ground carries on downwards until the coordinates
 * overflow. The floor is therefore a rule here and not a shape in the terrain.
 */
@Requires({
        @Facet(DensityFacet.class),
        @Facet(SurfacesFacet.class),
        @Facet(BiomeFacet.class),
        @Facet(SeaLevelFacet.class),
        @Facet(CaveFacet.class)})
public class CubeSolidRasterizer implements ScalableWorldRasterizer {

    private final CubeGeometry geometry;
    private Block water;
    private Block ice;
    private Block mantle;
    private Block lava;
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
        // Hardness zero, which is what Block.isDestructible reads: the one block the world is
        // allowed to end on. It had been sitting in CoreAssets describing itself as the bottom
        // layer of the world, and nothing had ever placed it.
        mantle = blockManager.getBlock("CoreAssets:MantleStone");
        // Luminance 15, the maximum the engine's light will carry, and like the mantle it had
        // never been placed by anything.
        lava = blockManager.getBlock("CoreAssets:Lava");
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
        CaveFacet caveFacet = chunkRegion.getFacet(CaveFacet.class);
        int seaLevel = chunkRegion.getFacet(SeaLevelFacet.class).getSeaLevel();

        Vector2i pos2d = new Vector2i();
        Vector3i worldPos = new Vector3i();
        for (Vector3ic pos : Chunks.CHUNK_REGION) {
            pos2d.set(pos.x(), pos.z());
            float density = solidityFacet.get(pos);
            // basePosY and not worldPos.y(): the latter counts in chunk units, so it is the scaled
            // figure on a level-of-detail chunk, and the world would end somewhere else out there.
            float basePosY = (pos.y() + chunk.getChunkWorldOffsetY()) * scale;
            Biome biome = biomeFacet.get(pos2d);
            // The decision lives in CubeCell, which the distant terrain asks too. What is left here
            // is turning it into blocks and biomes, and why each branch writes what it writes.
            CubeCell cell = CubeCell.classify(geometry, density, surfacesFacet.get(pos), caveFacet.get(pos),
                    basePosY, seaLevel, scale, biome);
            switch (cell) {
                case OUTSIDE:
                    // Off the world, above or below. No block, and no biome either: a biome hash of
                    // zero reads back as "nowhere", which is the truth here.
                    continue;
                case CAVE:
                case CAVE_LAVA:
                    // Ahead of every other branch, and the water branch in particular. An empty cell
                    // below the reference level is a cell of sea to the chain below, which is right
                    // for an ocean and would flood every gallery in the world: the sea sits at 32 and
                    // the floor of the world at -192. Nothing is written, the chunk having started as
                    // air.
                    biomeRegistry.setBiome(CoreBiome.UNDERGROUND, chunk, pos.x(), pos.y(), pos.z());
                    if (cell == CubeCell.CAVE_LAVA) {
                        // The deepest galleries are drowned in lava, which is the only light there is
                        // down here: a sealed cavity is pitch dark, there being no ambient minimum
                        // anywhere in the renderer. It is also the only reason a screenshot of the
                        // bottom of the world can be told from a screenshot of a generator that failed.
                        chunk.setBlock(pos, lava);
                    }
                    continue;
                default:
                    break;
            }
            biomeRegistry.setBiome(biome, chunk, pos.x(), pos.y(), pos.z());
            chunk.chunkToWorldPosition(pos, worldPos);
            Block block = blockFor(cell, biome, worldPos, seaLevel, density);
            if (block != null) {
                chunk.setBlock(pos, block);
            }
        }
    }

    /**
     * The block a cell becomes, or null for air. Public because the distant terrain names its
     * materials through this very table, never through a copy of it.
     * <p>
     * The floor is the whole of the lowest layer: the density there is huge and positive, so without
     * its own branch the world would simply carry on downwards for ever. And the caps carry their
     * own ice, and they are the only ones who know which: pack ice fractures, an ice shelf does not,
     * and the plain snow biome has neither. Asking the biome keeps that with the rest of the
     * material.
     */
    public Block blockFor(CubeCell cell, Biome biome, Vector3ic worldPos, int seaLevel, float density) {
        switch (cell) {
            case MANTLE:
                return mantle;
            case SEA_STRADDLE:
            case WATER:
                return water;
            case SURFACE:
                return biome.getSurfaceBlock(worldPos, seaLevel);
            case BELOW_SURFACE:
                return biome.getBelowSurfaceBlock(worldPos, density);
            case SEA_LID:
                return CoreBiome.SNOW == biome ? ice : biome.getSurfaceBlock(worldPos, seaLevel);
            case CAVE_LAVA:
                return lava;
            default:
                return null;
        }
    }
}
