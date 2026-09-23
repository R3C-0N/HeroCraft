// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector2f;
import org.joml.Vector2fc;
import org.joml.Vector3f;
import org.junit.jupiter.api.Test;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * A distant vertex is lit as the engine lights a vertex of a chunk: the same samples, the same
 * formula ({@code BlockMeshPart.appendLighting}), read off columns instead of blocks. The expected
 * numbers are the engine's, worked out by hand for the blocks each case stands for.
 */
class DistantLightTest {

    private static final Vector3f UP = new Vector3f(0, 1, 0);
    private static final Vector3f EAST = new Vector3f(1, 0, 0);
    private static final float EPSILON = 1e-5f;

    private static final int GRASS = 0;
    private static final int WATER = 1;

    /** Three by three columns two blocks wide, flat at ten but for the eastern row, at twelve. */
    private static DistantMesher.Columns step() {
        int size = 3;
        int[][] runs = new int[(size + 2) * (size + 2)][];
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                runs[(u + 1) + (v + 1) * (size + 2)] = new int[]{0, u == 2 ? 12 : 10, DistantMesher.SOLID};
            }
        }
        return new DistantMesher.Columns(size, 2, runs);
    }

    @Test
    void theFootOfAStepIsOccludedAsTheEngineOccludesIt() {
        DistantMesher.Columns columns = step();
        // The corner of the middle column's top against the step: two of its four blocks above are
        // the step's, so (0.4² + 0.8⁰) / 2.
        assertEquals((0.16f + 1.0f) / 2.0f, columns.occlusion(new Vector3f(4, 11, 2), UP), EPSILON);
        // A block in from the step, every block above is air.
        assertEquals(1.0f, columns.occlusion(new Vector3f(3, 11, 2), UP), EPSILON);
        // In the open, the full sky.
        assertEquals(1.0f, columns.sunlight(new Vector3f(3, 11, 3)), EPSILON);
    }

    @Test
    void theBaseOfAWallIsOccludedByTheGroundAtItsFoot() {
        DistantMesher.Columns columns = step();
        // The western wall of the step, facing west, at its foot: the two blocks below the vertex, in
        // the middle column and its neighbour along the wall, are ground.
        Vector3f west = new Vector3f(-1, 0, 0);
        assertEquals((0.16f + 1.0f) / 2.0f, columns.occlusion(new Vector3f(4, 11, 3), west), EPSILON);
        // At its top edge, all air.
        assertEquals(1.0f, columns.occlusion(new Vector3f(4, 13, 3), west), EPSILON);
        // Facing east from the middle column into the step it is buried: not a face that is drawn,
        // but the count says four.
        assertEquals((0.0256f + 1.0f) / 2.0f, columns.occlusion(new Vector3f(4, 11, 3), EAST), EPSILON);
    }

    @Test
    void theBedOfTheSeaIsDimmedByTheWaterAboveIt() {
        int size = 1;
        int[][] runs = new int[(size + 2) * (size + 2)][];
        for (int i = 0; i < runs.length; i++) {
            // Water from 20 to 29, on ground to 19: ten blocks of water.
            runs[i] = new int[]{20, 29, DistantMesher.WATER, 0, 19, DistantMesher.SOLID};
        }
        DistantMesher.Columns columns = new DistantMesher.Columns(size, 2, runs);
        // The surface block holds 14, one less per block down: 5 at the bottom block of the ten.
        assertEquals(5.0f / 15.0f, columns.sunlight(new Vector3f(1, 20, 1)), EPSILON);
        // Water is a translucent block that casts a shadow: four billboard occluders over the bed.
        assertEquals((1.0f + (float) Math.pow(0.8, 4)) / 2.0f, columns.occlusion(new Vector3f(1, 20, 1), UP), EPSILON);
        // The surface: the air above, and the surface block itself.
        assertEquals((4 * 15 + 4 * 14) / 8.0f / 15.0f, columns.sunlight(new Vector3f(1, 30, 1)), EPSILON);
    }

    @Test
    void waterGoesToTheRefractivePhaseAndItsBedIsDrawn() {
        // One column: ground to 19, water to 29.
        SectionKey key = new SectionKey(1, 1, 0, 0);
        int floor = -192;
        long[] segments = {
                ColumnSection.pack(20 - floor, 29 - floor, WATER, 15, 0, true),
                ColumnSection.pack(0, 19 - floor, GRASS, 15, 0, false)};
        ColumnSection section = new ColumnSection(key, 1, floor, new int[]{0, 2}, segments);
        ChunkMesh mesh = new DistantMesher(new Looks()).mesh(section, new int[]{0, 0}, (f, x, z, l) -> null);

        ChunkMesh.VertexElements water = mesh.getVertexElements(ChunkMesh.RenderType.WATER_AND_ICE);
        ChunkMesh.VertexElements ground = mesh.getVertexElements(ChunkMesh.RenderType.OPAQUE);
        assertEquals(4, water.vertexCount, "the surface of the water, one quad");
        // The bed: a top under the water, and the four skirts of an unknown neighbourhood.
        assertTrue(ground.vertexCount >= 4 * 5, "the bed is not drawn: " + ground.vertexCount);
    }

    @Test
    void aNodeIsDrawnHalfABlockBackAsTheEngineCentresItsBlocks() {
        DistantTerrain.Node node = new DistantTerrain.Node(new SectionKey(1, 1, 0, 0), null, new int[]{3200, 4800},
                null, 0, 10);
        assertEquals(3199.5f, node.renderPosition().x(), EPSILON);
        assertEquals(-0.5f, node.renderPosition().y(), EPSILON);
        assertEquals(4799.5f, node.renderPosition().z(), EPSILON);
        assertEquals(3200, node.gridX());
    }

    private static final class Looks implements DistantMesher.Looks {
        private final Vector2fc tile = new Vector2f();

        @Override
        public Vector2fc topTile(int material) {
            return tile;
        }

        @Override
        public Vector2fc sideTile(int material) {
            return tile;
        }

        @Override
        public Colorc topTint(int material) {
            return Color.white;
        }

        @Override
        public Colorc sideTint(int material) {
            return Color.white;
        }

        @Override
        public boolean isGrass(int material) {
            return material == GRASS;
        }

        @Override
        public boolean isWater(int material) {
            return material == WATER;
        }

        @Override
        public int frames(int material) {
            return 1;
        }
    }
}
