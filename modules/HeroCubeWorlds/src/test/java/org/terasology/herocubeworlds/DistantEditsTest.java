// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.distant.ColumnSection;
import org.terasology.herocubeworlds.distant.ColumnSummarizer;
import org.terasology.herocubeworlds.distant.DistantEdits;
import org.terasology.herocubeworlds.distant.DistantPalette;
import org.terasology.herocubeworlds.distant.SectionKey;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.chunks.ChunkProvider;
import org.terasology.engine.world.generator.WorldGenerator;

import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * A tower built in the world stands in the distance too.
 * <p>
 * On the real engine: the blocks are set through the world provider, read back from the loaded
 * chunk by {@link DistantEdits}, and the node over them is summarised again. Two by two so that a
 * level-one column, one block in four, lands on it; a narrower change may fall between the columns
 * of a coarse level, and that is the price named in {@link DistantEdits}.
 */
@IntegrationEnvironment(worldGenerator = "HeroCubeWorlds:cubeworld")
public class DistantEditsTest {

    private static final int HEIGHT = 24;

    @In
    private WorldProvider worldProvider;
    @In
    private MainLoop mainLoop;
    @In
    private ChunkProvider chunkProvider;
    @In
    private WorldGenerator worldGenerator;
    @In
    private BlockManager blockManager;

    @Test
    public void aTowerStandsInTheDistance() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        int[] centre = geometry.faceCentre(1);
        int x = centre[0];
        int z = centre[1];
        mainLoop.forceAndWaitForGeneration(new Vector3i(x, geometry.seaLevel(), z));
        int ground = topOf(x, z);
        assertTrue(ground > geometry.floorY(), "no ground to build on");
        // The chunks the tower rises into, which nobody asked for yet.
        for (int y = ground; y <= ground + HEIGHT; y += 16) {
            mainLoop.forceAndWaitForGeneration(new Vector3i(x, y, z));
        }

        CubeSolidRasterizer rasterizer = new CubeSolidRasterizer(geometry);
        rasterizer.initialize();
        DistantPalette palette = new DistantPalette(rasterizer);
        DistantEdits edits = new DistantEdits();
        ColumnSummarizer withEdits = new ColumnSummarizer(((CubeWorldGenerator) worldGenerator).getWorld(),
                geometry, palette, edits);

        SectionKey key = keyAt(geometry, 1, x, z);
        int u = (CubePattern.localU(1, x, geometry.faceBlocks()) % key.span(geometry)) / key.stride();
        int v = (CubePattern.localV(1, z, geometry.faceBlocks()) % key.span(geometry)) / key.stride();
        int before = topOf(withEdits.summarize(key), u, v);

        Block stone = blockManager.getBlock("CoreAssets:Stone");
        Vector3i pos = new Vector3i();
        for (int dx = 0; dx < 2; dx++) {
            for (int dz = 0; dz < 2; dz++) {
                for (int y = ground + 1; y <= ground + HEIGHT; y++) {
                    worldProvider.setBlock(pos.set(x + dx, y, z + dz), stone);
                    edits.note(pos);
                }
            }
        }
        Set<Long> changed = edits.capture(chunkProvider, palette);
        int after = topOf(withEdits.summarize(key), u, v);
        System.out.printf("sol a %d ; colonne de niveau 1 avant : %d, apres la tour : %d ; %d chunk(s) capture(s)%n",
                ground, before, after, edits.capturedChunks());

        assertFalse(changed.isEmpty(), "nothing was captured");
        assertTrue(Math.abs(before - ground) <= key.stride(), "the untouched column is not the ground");
        // A level-one cell is two blocks tall: the top of the tower lands anywhere in its cell.
        assertTrue(after >= ground + HEIGHT && after < ground + HEIGHT + key.stride(),
                "the tower does not stand in the distance: " + after);
    }

    private static SectionKey keyAt(CubeGeometry geometry, int level, int x, int z) {
        SectionKey probe = new SectionKey(1, level, 0, 0);
        int span = probe.span(geometry);
        return new SectionKey(1, level, CubePattern.localU(1, x, geometry.faceBlocks()) / span,
                CubePattern.localV(1, z, geometry.faceBlocks()) / span);
    }

    private int topOf(int x, int z) {
        for (int y = 160; y > CubeGeometry.DEFAULT_FLOOR_Y; y--) {
            Block block = worldProvider.getBlock(x, y, z);
            // An unloaded cell answers a block that is not penetrable: it is not ground.
            if (!block.isPenetrable() && !"engine:unloaded".equals(block.getURI().toString())) {
                return y;
            }
        }
        return CubeGeometry.DEFAULT_FLOOR_Y;
    }

    private static int topOf(ColumnSection section, int u, int v) {
        for (int k = 0; k < section.count(u, v); k++) {
            long s = section.segment(u, v, k);
            if (!ColumnSection.isLiquid(s)) {
                return section.top(s);
            }
        }
        return Integer.MIN_VALUE;
    }
}
