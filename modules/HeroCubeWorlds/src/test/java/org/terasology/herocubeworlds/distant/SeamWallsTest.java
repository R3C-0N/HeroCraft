// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector2f;
import org.joml.Vector2fc;
import org.joml.Vector3f;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.herocubeworlds.generator.rasterizers.CubeCell;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FacePos;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

import java.util.HashMap;
import java.util.Locale;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the distance builds on the edge of a node that lies on a cut seam: nothing, when the node across is there.
 * <p>
 * A cut seam is an edge the net leaves open against a dead slot and glues back only on the sphere, so the column
 * across it is asked through the folding and never next door in the grid. If the folding answered with the wrong
 * column, the two sides would disagree about their ground and wall each other off along the whole seam. The glued
 * edges are counted beside them as the witness that the count sees walls at all: there the ground has real cliffs.
 * <p>
 * This is the measure that cleared the distance of the wall seen standing down the seam from a corner. The wall was
 * not built here: it was the outer corners of the seam blocks, half a block into the dead slot and thrown across the
 * planet by the projection (see CutSeamMeetsTest).
 */
class SeamWallsTest {

    private static CubeGeometry geometry;
    private static ColumnSummarizer summarizer;
    private static final Map<SectionKey, ColumnSection> SECTIONS = new HashMap<>();

    private static final DistantMesher MESHER = new DistantMesher(new DistantMesher.Looks() {
        public Vector2fc topTile(int material) {
            return new Vector2f();
        }

        public Vector2fc sideTile(int material) {
            return new Vector2f();
        }

        public Colorc topTint(int material) {
            return Color.white;
        }

        public Colorc sideTint(int material) {
            return Color.white;
        }

        public boolean isGrass(int material) {
            return false;
        }

        public boolean isWater(int material) {
            return material == 1;
        }

        public int frames(int material) {
            return 0;
        }
    });

    @BeforeAll
    static void buildChain() {
        geometry = CubeGeometry.createDefault();
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry));
        builder.setSeed(0xC0FFEE);
        World world = builder.build();
        summarizer = new ColumnSummarizer(world, geometry,
                (cell, biome, pos, sea, density) -> cell == CubeCell.WATER || cell == CubeCell.SEA_STRADDLE ? 1 : 2);
    }

    private static ColumnSection section(SectionKey key) {
        return SECTIONS.computeIfAbsent(key, summarizer::summarize);
    }

    /** DistantTerrain.runs, with every node of the asked level built. */
    private static int[] runs(int face, int netX, int netZ, int level) {
        FacePos pos = geometry.fold(face, netX + 0.5, netZ + 0.5);
        if (pos.face() == CubePattern.NO_FACE) {
            return null;
        }
        int u = (int) Math.floor(pos.u());
        int v = (int) Math.floor(pos.v());
        int span = SectionKey.columns(geometry) << level;
        int stride = 1 << level;
        return MESHER.runs(section(new SectionKey(pos.face(), level, u / span, v / span)), (u % span) / stride,
                (v % span) / stride);
    }

    /** Wall quads a node builds on one edge, facing out of it. */
    private static int wallsOnEdge(SectionKey key, float nx, float nz, boolean atMax) {
        ChunkMesh mesh = MESHER.mesh(section(key), key.netOrigin(geometry), SeamWallsTest::runs);
        ChunkMesh.VertexElements ground = mesh.getVertexElements(ChunkMesh.RenderType.OPAQUE);
        float edge = atMax ? key.span(geometry) : 0;
        Vector3f p = new Vector3f();
        Vector3f n = new Vector3f();
        int vertices = 0;
        for (int i = 0; i < ground.position.elements(); i++) {
            ground.normals.get(i, n);
            ground.position.get(i, p);
            float along = nx != 0 ? p.x : p.z;
            if (Math.abs(n.x - nx) < 1e-3 && Math.abs(n.z - nz) < 1e-3 && Math.abs(along - edge) < 1e-3) {
                vertices++;
            }
        }
        return vertices / 4;
    }

    @Test
    void aCutSeamBuildsNoWallAgainstTheNodeAcross() {
        int level = 1;
        int last = SectionKey.nodesPerSide(level) - 1;
        int cut = 0;
        int glued = 0;
        // Face 0 sits in slot (0, 1): its first row lies against the dead slot (0, 0), cut, and glued to face 5 on
        // the sphere; its last column lies against face 1, glued in the grid itself.
        for (int i = 4; i <= last; i += 9) {
            cut += wallsOnEdge(new SectionKey(0, level, i, 0), 0, -1, false);
            glued += wallsOnEdge(new SectionKey(0, level, last, i), 1, 0, true);
        }
        System.out.printf(Locale.ROOT, "walls on four nodes of a cut seam: %d; of a glued one: %d%n", cut, glued);
        assertTrue(glued > 0, "no wall on a glued edge either: the count is not looking at the edge");
        assertEquals(0, cut, "the two sides of a cut seam disagree about their ground");
    }
}
