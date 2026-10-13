// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.chunks.Chunk;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

/**
 * The hole: the columns the chunks draw, and the ring round the player, kept clear with no chunk
 * drawn in it, stopped at the dead slots of the net, and given up when asked to.
 */
class DistantHoleTest {

    private static final List<Chunk> NO_CHUNK = List.of();

    private final CubeGeometry geometry = CubeGeometry.createDefault();

    @Test
    void theRingIsKeptClearBeforeAnyChunkIsDrawn() {
        DistantHole hole = new DistantHole(geometry);
        int[] centre = centreChunk(0);
        hole.mark(NO_CHUNK, centre[0], centre[1], 3);
        assertEquals(49, hole.keptColumns());
        assertEquals(0, hole.drawnColumns());
        assertTrue(hole.covers(centre[0] * 32, centre[1] * 32, 32));
        assertFalse(hole.covers((centre[0] + 3) * 32, centre[1] * 32, 64));
    }

    @Test
    void aNegativeRadiusKeepsNothing() {
        DistantHole hole = new DistantHole(geometry);
        int[] centre = centreChunk(0);
        hole.mark(NO_CHUNK, centre[0], centre[1], -1);
        assertEquals(0, hole.keptColumns());
        assertFalse(hole.covers(centre[0] * 32, centre[1] * 32, 32));
    }

    @Test
    void aDrawnColumnIsNotCountedAsKept() {
        DistantHole hole = new DistantHole(geometry);
        int[] centre = centreChunk(0);
        // A column loaded from chunk 1 to chunk 4: 1 and 4 are never meshed, and 2 and 3 are.
        List<Chunk> chunks = new ArrayList<>();
        for (int y = 1; y <= 4; y++) {
            chunks.add(chunk(centre[0], y, centre[1], y > 1 && y < 4));
        }
        hole.mark(chunks, centre[0], centre[1], 1);
        assertEquals(1, hole.drawnColumns());
        assertEquals(8, hole.keptColumns());
    }

    @Test
    void aColumnStillWaitingForAMiddleChunkIsNotCut() {
        DistantHole hole = new DistantHole(geometry);
        int[] centre = centreChunk(0);
        List<Chunk> chunks = new ArrayList<>();
        for (int y = 1; y <= 4; y++) {
            chunks.add(chunk(centre[0], y, centre[1], y == 2));
        }
        hole.mark(chunks, centre[0], centre[1], -1);
        assertEquals(0, hole.drawnColumns());
    }

    @Test
    void theRingStopsAtTheDeadSlots() {
        DistantHole hole = new DistantHole(geometry);
        int faceChunks = geometry.faceChunks();
        // The ring round the corner chunk of every face: whatever falls on a dead slot is left out.
        boolean someLeftOut = false;
        for (int face = 0; face < 6; face++) {
            int[] centre = centreChunk(face);
            int cornerX = centre[0] - faceChunks / 2;
            int cornerZ = centre[1] - faceChunks / 2;
            hole.mark(NO_CHUNK, cornerX, cornerZ, 3);
            int expected = 0;
            for (int z = cornerZ - 3; z <= cornerZ + 3; z++) {
                for (int x = cornerX - 3; x <= cornerX + 3; x++) {
                    if (x >= 0 && z >= 0 && geometry.faceOf(x * 32 + 16, z * 32 + 16) != CubePattern.NO_FACE) {
                        expected++;
                    }
                }
            }
            someLeftOut |= expected < 49;
            assertEquals(expected, hole.keptColumns(), "face " + face);
        }
        assertTrue(someLeftOut, "no ring met a dead slot");
    }

    private int[] centreChunk(int face) {
        int[] block = geometry.faceCentre(face);
        return new int[]{Math.floorDiv(block[0], 32), Math.floorDiv(block[1], 32)};
    }

    private static Chunk chunk(int x, int y, int z, boolean meshed) {
        Chunk chunk = mock(Chunk.class);
        when(chunk.getPosition()).thenReturn(new Vector3i(x, y, z));
        when(chunk.isReady()).thenReturn(true);
        when(chunk.hasMesh()).thenReturn(meshed);
        if (meshed) {
            ChunkMesh mesh = mock(ChunkMesh.class);
            when(mesh.triangleCount(ChunkMesh.RenderPhase.OPAQUE)).thenReturn(12);
            when(chunk.getMesh()).thenReturn(mesh);
        }
        return chunk;
    }
}
