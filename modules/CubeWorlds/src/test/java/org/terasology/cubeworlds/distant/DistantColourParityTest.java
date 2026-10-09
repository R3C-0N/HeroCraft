// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.rendering.primitives.ChunkTessellator;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.nui.Color;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.cubeworlds.generator.CubeWorldGenerator;
import org.terasology.engine.utilities.Assets;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generator.WorldGenerator;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The distance tints a block as a chunk does. The colour a chunk gives the vertices of a block is
 * read off a mesh the engine made, and the colour of the block's top in the distance has to be among
 * them — grass and leaves included, whose colour is the biome's.
 */
@IntegrationEnvironment(worldGenerator = "CubeWorlds:cubeworld")
public class DistantColourParityTest {

    @In
    private WorldProvider worldProvider;
    @In
    private MainLoop mainLoop;
    @In
    private BlockManager blockManager;
    @In
    private WorldGenerator worldGenerator;

    @Test
    public void aDistantVertexCarriesTheColourOfAChunkVertex() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        int[] centre = geometry.faceCentre(1);
        Vector3i pos = new Vector3i(centre[0] + 5, geometry.seaLevel() + 40, centre[1] + 5);
        mainLoop.forceAndWaitForGeneration(pos);
        CubeSolidRasterizer rasterizer = new CubeSolidRasterizer(geometry);
        rasterizer.initialize();
        DistantPalette palette = new DistantPalette(rasterizer);
        palette.setBiomeColours(Assets.getTexture("engine:grasscolor").get().getData(),
                Assets.getTexture("engine:foliagecolor").get().getData());
        Biome biome = ((CubeWorldGenerator) worldGenerator).getWorld()
                .getWorldData(new BlockRegion(pos), 1).getFacet(BiomeFacet.class).getWorld(pos.x, pos.z);
        for (String name : new String[]{"CoreAssets:LightLeaf", "CoreAssets:DarkLeaf", "CoreAssets:GreenLeaf",
                "CoreAssets:RedLeaf", "CoreAssets:Grass"}) {
            Block block = blockManager.getBlock(name);
            worldProvider.setBlock(pos, block);
            Vector3i chunkPos = new Vector3i(Math.floorDiv(pos.x, Chunks.SIZE_X), Math.floorDiv(pos.y, Chunks.SIZE_Y),
                    Math.floorDiv(pos.z, Chunks.SIZE_Z));
            ChunkMesh mesh = new ChunkTessellator().generateMesh(worldProvider.getLocalView(chunkPos));
            StringBuilder seen = new StringBuilder();
            for (ChunkMesh.RenderType type : ChunkMesh.RenderType.values()) {
                ChunkMesh.VertexElements elements = mesh.getVertexElements(type);
                Color colour = new Color();
                java.util.Set<String> colours = new java.util.TreeSet<>();
                for (int i = 0; i < elements.vertexCount; i++) {
                    elements.color.get(i, colour);
                    colours.add(String.format("(%.2f,%.2f,%.2f,%.2f)", colour.rf(), colour.gf(), colour.bf(), colour.af()));
                }
                if (!colours.isEmpty()) {
                    seen.append(' ').append(type).append('=').append(colours);
                }
            }
            int id = palette.tinted(palette.idOf(block), biome);
            Color top = new Color(palette.topTint(id));
            System.out.printf("%s (%s) : chunk%s | lointain (%.2f,%.2f,%.2f,%.2f)%n", name, biome, seen, top.rf(), top.gf(),
                    top.bf(), top.af());
            // The top of the block as the chunk made it: the colour of its top face is among the chunk's.
            String expected = String.format("(%.2f,%.2f,%.2f,%.2f)", top.rf(), top.gf(), top.bf(), top.af());
            assertTrue(seen.toString().contains(expected), name + " is not tinted as a chunk tints it: " + expected
                    + " against" + seen);
        }
    }
}
