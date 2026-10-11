// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.generator.WorldGenerator;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The trees of the distance are the trees of the world.
 * <p>
 * On the real engine: a patch of three by three chunks is generated, and every block the distant
 * trees put in a sampled cell is compared with the block the game generated there; and every trunk
 * or leaf the game generated at a sampled cell has to be among the distant ones. Not to the last
 * block: two trees can overlap, and which one wins a cell depends on the order a chunk grows them.
 */
@IntegrationEnvironment(worldGenerator = "HeroCubeWorlds:cubeworld")
public class DistantTreesTest {

    private static final int STRIDE = 2;
    private static final int CHUNKS = 3;
    private static final float AGREEMENT = 0.95f;
    /** Trees a patch needs to count as wooded, and how many patches are tried to find one. */
    private static final int WOODED = 12;
    private static final int SEARCH_STEPS = 40;

    @In
    private WorldProvider worldProvider;
    @In
    private MainLoop mainLoop;
    @In
    private WorldGenerator worldGenerator;
    @In
    private BlockManager blockManager;

    @Test
    public void theDistantTreesAreTheTreesOfTheWorld() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        int side = CHUNKS * Chunks.SIZE_X;
        int sea = geometry.seaLevel();
        CubeSolidRasterizer rasterizer = new CubeSolidRasterizer(geometry);
        rasterizer.initialize();
        DistantPalette palette = new DistantPalette(rasterizer);
        DistantTrees trees = new DistantTrees(((CubeWorldGenerator) worldGenerator).getWorld(), blockManager, palette);

        // A wooded patch: walked to from the centre of a face, asking the distant trees only, which is
        // cheap, before anything is generated.
        int[] centre = geometry.faceCentre(1);
        int minX = 0;
        int minZ = 0;
        DistantTrees.Cells cells = null;
        long elapsed = 0;
        for (int step = 0; step < SEARCH_STEPS; step++) {
            minX = Math.floorDiv(centre[0] + step * side, Chunks.SIZE_X) * Chunks.SIZE_X;
            minZ = Math.floorDiv(centre[1] + (step % 2) * side, Chunks.SIZE_Z) * Chunks.SIZE_Z;
            long start = System.nanoTime();
            cells = trees.plant(minX, minZ, side / STRIDE, STRIDE, sea, sea + 80);
            elapsed = System.nanoTime() - start;
            if (cells.trees() >= WOODED) {
                break;
            }
        }
        for (int cx = 0; cx < CHUNKS; cx++) {
            for (int cz = 0; cz < CHUNKS; cz++) {
                for (int y = sea - 16; y <= sea + 112; y += Chunks.SIZE_Y) {
                    mainLoop.forceAndWaitForGeneration(new Vector3i(minX + cx * Chunks.SIZE_X + 1, y,
                            minZ + cz * Chunks.SIZE_Z + 1));
                }
            }
        }

        int distant = 0;
        int agreeing = 0;
        int real = 0;
        int found = 0;
        int columnsUnderLeaves = 0;
        for (int u = 0; u < side / STRIDE; u++) {
            for (int v = 0; v < side / STRIDE; v++) {
                boolean underLeaves = false;
                for (int y = sea / STRIDE; y <= (sea + 110) / STRIDE; y++) {
                    Block block = worldProvider.getBlock(minX + u * STRIDE, y * STRIDE, minZ + v * STRIDE);
                    int material = cells.material(u, v, y);
                    boolean isTree = isTreeBlock(block);
                    if (material >= 0) {
                        distant++;
                        if (palette.block(material) == block) {
                            agreeing++;
                        }
                    }
                    if (isTree) {
                        real++;
                        underLeaves |= block.getURI().toString().contains("Leaf");
                        if (material >= 0) {
                            found++;
                        }
                    }
                }
                if (underLeaves) {
                    columnsUnderLeaves++;
                }
            }
        }
        // The coarse levels' estimate of the same ground, from the biome at the middle of the patch.
        Biome biome = ((CubeWorldGenerator) worldGenerator).getWorld()
                .getWorldData(new BlockRegion(minX + side / 2, sea, minZ + side / 2), 1)
                .getFacet(BiomeFacet.class).getWorld(minX + side / 2, minZ + side / 2);
        DistantTrees.Canopy canopy = trees.canopy(biome);
        double realCover = columnsUnderLeaves / (double) ((side / STRIDE) * (side / STRIDE));
        System.out.printf("biome %s : sous les feuilles %.0f %% des colonnes, canopee estimee %.0f %%%n",
                biome, 100 * realCover, canopy == null ? 0.0 : 100 * canopy.cover());
        System.out.printf("parcelle %d,%d : %d arbre(s), %d cellule(s) d'arbre au loin dont %d identiques ; %d au monde dont %d trouvees ;"
                + " %.0f ms%n", minX, minZ, cells.trees(), distant, agreeing, real, found, elapsed / 1e6);
        assertTrue(real > 0, "no tree in the patch: the test proves nothing here");
        assertTrue(agreeing >= AGREEMENT * distant, "the distant trees are not the world's: " + agreeing + "/" + distant);
        assertTrue(found >= AGREEMENT * real, "trees of the world are missing from the distance: " + found + "/" + real);
        assertTrue(canopy != null && canopy.cover() > realCover / 2 && canopy.cover() < realCover * 2,
                "the canopy of the coarse levels is not the cover of the trees: " + (canopy == null ? 0 : canopy.cover())
                        + " against " + realCover);
    }

    private static boolean isTreeBlock(Block block) {
        String uri = block.getURI().toString().toLowerCase();
        return uri.contains("leaf") || uri.contains("trunk") || uri.contains("cactus");
    }
}
