// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector2f;
import org.joml.Vector2fc;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.generator.rasterizers.CubeCell;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

import java.util.Locale;

/**
 * What a distant node costs to draw: its triangles, for the ground and for the water. A census, on
 * the generator alone: it prints, it does not judge.
 */
class DistantMeshCostTest {

    private static CubeGeometry geometry;
    private static ColumnSummarizer summarizer;

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
        summarizer = new ColumnSummarizer(world, geometry, (cell, biome, pos, sea, density) -> cell.ordinal());
    }

    @Test
    void trianglesPerNode() {
        Looks looks = new Looks();
        DistantMesher mesher = new DistantMesher(looks);
        System.out.printf(Locale.ROOT, "%6s %6s | %10s %10s | %10s%n", "niveau", "noeud", "sol", "eau", "par colonne");
        for (int level = 1; level <= 4; level++) {
            int n = SectionKey.nodesPerSide(level);
            for (int[] at : new int[][]{{n / 2, n / 2}, {n / 3, 2 * n / 3}}) {
                SectionKey key = new SectionKey(1, level, at[0], at[1]);
                ColumnSection section = summarizer.summarize(key);
                int[] origin = key.netOrigin(geometry);
                ChunkMesh mesh = mesher.mesh(section, origin, (f, x, z, l) -> null);
                int ground = mesh.getVertexElements(ChunkMesh.RenderType.OPAQUE).indices.indices() / 3;
                int water = mesh.getVertexElements(ChunkMesh.RenderType.WATER_AND_ICE).indices.indices() / 3;
                System.out.printf(Locale.ROOT, "%6d %6s | %10d %10d | %10.1f%n", level, at[0] + "," + at[1],
                        ground, water, ground / (double) (section.size() * section.size()));
            }
        }
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
            return material == CubeCell.SURFACE.ordinal();
        }

        @Override
        public boolean isWater(int material) {
            return material == CubeCell.WATER.ordinal() || material == CubeCell.SEA_LID.ordinal();
        }

        @Override
        public int frames(int material) {
            return 1;
        }
    }
}
