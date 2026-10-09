// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector2fc;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.rendering.primitives.ChunkMeshImpl;
import org.terasology.engine.rendering.primitives.ChunkVertexFlag;
import org.terasology.engine.rendering.primitives.WaterDepthField;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

/**
 * Turns a node of columns into a chunk mesh: the top of every run, and the walls a run shows to its
 * four neighbours.
 * <p>
 * The mesh is an ordinary {@link ChunkMesh}, in the chunk's own vertex format, and it is drawn by the
 * chunk shader. That is the whole point: a distant vertex is placed on the sphere by the very
 * function that places a vertex of the terrain, from the same kind of input — a position on the net,
 * relative to an origin on the net — so the two cannot disagree about where the world is.
 * <p>
 * It is lit as the terrain is, too. The occlusion and the sunlight of every vertex are read off the
 * columns around it with the samples and the formula of the engine's own mesher
 * ({@code BlockMeshPart.appendLighting}), so a step, the foot of a cliff or the bed of the sea darkens
 * as much in all as it does up close — spread over a column rather than a block, since a column is a
 * single quad (see {@link #patch}). Water is not drawn with the ground: its surface goes to the
 * refractive phase, where the water pass shows the bed through it.
 * <p>
 * No triangle crosses a seam. A node lies on one face, and a wall is always built on the node's own
 * side of its edge, down to what the neighbour shows; the neighbour is read through
 * {@link Neighbours}, which folds across the seam. What lies across is asked, never computed from a
 * difference of coordinates.
 */
public final class DistantMesher {

    /** The runs a neighbouring column shows, top first, as {@link #runs} gives them. */
    @FunctionalInterface
    public interface Neighbours {
        /**
         * @param netX a column of the net, possibly past the edge of the node's face
         * @param netZ likewise
         * @return the runs of the column there, or null if nothing is known about it yet
         */
        int[] runs(int face, int netX, int netZ, int level);
    }

    /** How a material looks: where its faces are in the atlas, and their tint. */
    public interface Looks {
        Vector2fc topTile(int material);

        Vector2fc sideTile(int material);

        Colorc topTint(int material);

        Colorc sideTint(int material);

        /** Grass carries its tint in a mask on its sides, and the shader wants to be told. */
        boolean isGrass(int material);

        /** Water is seen through, and drawn by the refractive pass; any other liquid is ground that glows. */
        boolean isWater(int material);

        /** The frames of the material's tile: more than one for an animated one, like lava. */
        int frames(int material);
    }

    /** What a run is to the light: ground that hides it, water that dims it, or a liquid that does neither. */
    static final int SOLID = 1;
    static final int WATER = 2;
    static final int OTHER_LIQUID = 3;
    /** A run, in {@link #runs}: bottom, top, kind. */
    static final int RUN = 3;

    /** The top of a water block, below the top of its cell: the engine's lowered cube. */
    private static final float WATER_SURFACE_DROP = 0.1f;
    private static final int FULL_LIGHT = 15;

    /** The engine's samples, in {@code BlockMeshPart}: at a vertex, then eight tenths above and a tenth below. */
    private static final float[] LIGHT_SAMPLE_X = {0.1f, 0.1f, -0.1f, -0.1f, 0.1f, 0.1f, -0.1f, -0.1f};
    private static final float[] LIGHT_SAMPLE_Y = {0.8f, 0.8f, 0.8f, 0.8f, -0.1f, -0.1f, -0.1f, -0.1f};
    private static final float[] LIGHT_SAMPLE_Z = {0.1f, -0.1f, -0.1f, 0.1f, 0.1f, -0.1f, -0.1f, 0.1f};
    private static final double[] OPAQUE_OCCLUSION = occlusionTable(0.40);
    private static final double[] BILLBOARD_OCCLUSION = occlusionTable(0.80);

    private static final Vector3f UP = new Vector3f(0, 1, 0);
    private static final Vector3f DOWN = new Vector3f(0, -1, 0);
    private static final Vector3f[] SIDE_NORMALS = {
            new Vector3f(-1, 0, 0), new Vector3f(1, 0, 0), new Vector3f(0, 0, -1), new Vector3f(0, 0, 1)};
    private static final int[][] SIDE_STEP = {{-1, 0}, {1, 0}, {0, -1}, {0, 1}};
    /** Along each wall, the two directions that span it, as the quad is wound: down, then across. */
    private static final Vector3f[] SIDE_ACROSS = {
            new Vector3f(0, 0, 1), new Vector3f(0, 0, -1), new Vector3f(-1, 0, 0), new Vector3f(1, 0, 0)};
    private static final Vector3f X_AXIS = new Vector3f(1, 0, 0);
    private static final Vector3f Z_AXIS = new Vector3f(0, 0, 1);
    private static final int SKIRT_COLUMNS = 4;

    private final Looks looks;

    public DistantMesher(Looks looks) {
        this.looks = looks;
    }

    /**
     * @param section the node
     * @param origin the net column of its first column, as {x, z}
     * @param neighbours what lies around it
     * @return the mesh, with vertices relative to {@code origin} at height zero, in the grid of the
     *         world: a block is the unit cell above its coordinates, not centred on them
     */
    public ChunkMesh mesh(ColumnSection section, int[] origin, Neighbours neighbours) {
        ChunkMeshImpl mesh = new ChunkMeshImpl();
        ChunkMesh.VertexElements ground = mesh.getVertexElements(ChunkMesh.RenderType.OPAQUE);
        ChunkMesh.VertexElements water = mesh.getVertexElements(ChunkMesh.RenderType.WATER_AND_ICE);
        Columns columns = new Columns(section, origin, neighbours);
        int size = section.size();
        int stride = section.key().stride();
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                columns.fill(u, v, runs(section, u, v));
            }
        }
        Vector3f corner = new Vector3f();
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                float x0 = u * stride;
                float z0 = v * stride;
                int count = section.count(u, v);
                for (int k = 0; k < count; k++) {
                    long seg = section.segment(u, v, k);
                    int material = ColumnSection.material(seg);
                    int kind = kindOf(seg);
                    float block = ColumnSection.blockLight(seg) / 15f;
                    int top = section.top(seg);
                    // A top is drawn unless ground sits right on it: under water it is the bed of the sea.
                    boolean covered = k > 0 && section.bottom(section.segment(u, v, k - 1)) == top + 1
                            && kindOf(section.segment(u, v, k - 1)) == SOLID;
                    if (!covered) {
                        corner.set(x0, top + 1, z0);
                        if (kind == WATER) {
                            float depth = Math.min(1.0f, (top - section.bottom(seg) + 1) / WaterDepthField.RANGE);
                            patch(water, columns, corner, Z_AXIS, stride, X_AXIS, stride, UP,
                                    looks.topTile(material), looks.topTint(material), ChunkVertexFlag.WATER_SURFACE,
                                    material, block, (byte) Math.round(depth * 127.0f), WATER_SURFACE_DROP);
                        } else {
                            patch(ground, columns, corner, Z_AXIS, stride, X_AXIS, stride, UP,
                                    looks.topTile(material), looks.topTint(material), ChunkVertexFlag.NORMAL,
                                    material, block, (byte) 0, 0);
                        }
                    }
                    if (kind != SOLID) {
                        continue;
                    }
                    for (int side = 0; side < 4; side++) {
                        int[] other = columns.at(u + SIDE_STEP[side][0], v + SIDE_STEP[side][1]);
                        if (other == null) {
                            // Unknown ground next door: a skirt, four columns deep, rather than a wall
                            // to the floor of the world that the neighbour will cover a moment later.
                            // It hides the crack between two nodes whose heights disagree.
                            emitWall(ground, columns, side, u, v, Math.max(section.bottom(seg),
                                    top - SKIRT_COLUMNS * stride), top, material, block);
                            continue;
                        }
                        wall(ground, columns, side, u, v, section.bottom(seg), top, other, material, block);
                    }
                }
            }
        }
        return mesh;
    }

    /** The parts of a run no neighbouring ground hides, as walls facing that neighbour. */
    private void wall(ChunkMesh.VertexElements out, Columns columns, int side, int u, int v,
                      int bottom, int top, int[] other, int material, float block) {
        int from = top;
        // Walk down the run, skipping what the neighbour covers.
        for (int k = 0; k < other.length && from >= bottom; k += RUN) {
            if (other[k + 2] != SOLID) {
                continue;
            }
            int otherBottom = other[k];
            int otherTop = other[k + 1];
            if (otherTop < bottom) {
                break;
            }
            if (otherTop < from) {
                emitWall(out, columns, side, u, v, Math.max(otherTop + 1, bottom), from, material, block);
            }
            from = Math.min(from, otherBottom - 1);
        }
        if (from >= bottom) {
            emitWall(out, columns, side, u, v, bottom, from, material, block);
        }
    }

    private void emitWall(ChunkMesh.VertexElements out, Columns columns, int side, int u, int v,
                          int low, int high, int material, float block) {
        int stride = columns.stride;
        float x0 = u * stride;
        float z0 = v * stride;
        float x1 = x0 + stride;
        float z1 = z0 + stride;
        float y1 = high + 1;
        Vector3f corner = new Vector3f();
        switch (side) {
            case 0:
                corner.set(x0, y1, z0);
                break;
            case 1:
                corner.set(x1, y1, z1);
                break;
            case 2:
                corner.set(x1, y1, z0);
                break;
            default:
                corner.set(x0, y1, z1);
                break;
        }
        ChunkVertexFlag flag = looks.isGrass(material) ? ChunkVertexFlag.COLOR_MASK : ChunkVertexFlag.NORMAL;
        patch(out, columns, corner, DOWN, high + 1 - low, SIDE_ACROSS[side], stride, SIDE_NORMALS[side],
                looks.sideTile(material), looks.sideTint(material), flag, material, block, (byte) 0, 0);
    }

    /**
     * A rectangle from {@code corner}, {@code lengthA} blocks along {@code a} and {@code lengthB} along
     * {@code b}, wound counter-clockwise seen from {@code normal} (a × b is the normal): one quad.
     * <p>
     * The engine darkens a strip one block wide along an occluded edge, fading across that block. A
     * quad here is several blocks wide, and interpolates its corners across all of them; so a corner
     * carries the occlusion divided by the quad's width, and the quad darkens by as much in all as the
     * strip would have. Cutting the strips into the mesh instead cost four to eight times the triangles,
     * for a band a pixel wide at the distances these quads are drawn at.
     *
     * @param drop how far the drawn surface sits below the one the light is sampled on
     */
    private void patch(ChunkMesh.VertexElements out, Columns columns, Vector3fc corner,
                       Vector3fc a, int lengthA, Vector3fc b, int lengthB, Vector3f normal,
                       Vector2fc tile, Colorc tint, ChunkVertexFlag flag, int material, float block,
                       byte waterDepth, float drop) {
        float width = Math.max(1, Math.min(lengthA, lengthB));
        int frames = looks.frames(material);
        int base = out.vertexCount;
        Vector3f p = new Vector3f();
        Vector3f drawn = new Vector3f();
        for (int i = 0; i < 4; i++) {
            p.set(corner).fma(i == 1 || i == 2 ? lengthA : 0, a).fma(i >= 2 ? lengthB : 0, b);
            float occlusion = 1.0f - (1.0f - columns.occlusion(p, normal)) / width;
            out.position.put(drawn.set(p.x, p.y - drop, p.z));
            out.normals.put(normal);
            out.uv0.put(tile);
            out.flags.put((byte) flag.getValue());
            out.frames.put((byte) (frames - 1));
            out.waterDepth.put(waterDepth);
            out.color.put(tint);
            out.sunlight.put(columns.sunlight(p));
            out.blockLight.put(block);
            out.warmth.put((byte) (block > 0 ? 127 : 0));
            out.ambientOcclusion.put(occlusion);
        }
        out.vertexCount += 4;
        out.indices.put(base);
        out.indices.put(base + 1);
        out.indices.put(base + 2);
        out.indices.put(base + 2);
        out.indices.put(base + 3);
        out.indices.put(base);
    }

    /** The runs of a column of the node, top first, {@link #RUN} numbers each: bottom, top and kind. */
    public int[] runs(ColumnSection section, int u, int v) {
        int count = section.count(u, v);
        int[] runs = new int[RUN * count];
        for (int k = 0; k < count; k++) {
            long seg = section.segment(u, v, k);
            runs[RUN * k] = section.bottom(seg);
            runs[RUN * k + 1] = section.top(seg);
            runs[RUN * k + 2] = kindOf(seg);
        }
        return runs;
    }

    private int kindOf(long segment) {
        if (!ColumnSection.isLiquid(segment)) {
            return SOLID;
        }
        return looks.isWater(ColumnSection.material(segment)) ? WATER : OTHER_LIQUID;
    }

    private static double[] occlusionTable(double base) {
        double[] table = new double[5];
        for (int occluders = 0; occluders < table.length; occluders++) {
            table[occluders] = Math.pow(base, occluders);
        }
        return table;
    }

    /**
     * The columns of a node and the ring around it, as runs, and what a block of them is to the light.
     * Coordinates are in blocks from the node's origin, in the grid of the world: the cell of a block
     * is the unit above its coordinates. A column nothing is known of is open sky.
     */
    static final class Columns {
        private final int size;
        private final int stride;
        private final int[][] runs;

        Columns(int size, int stride, int[][] runs) {
            this.size = size;
            this.stride = stride;
            this.runs = runs;
        }

        Columns(ColumnSection section, int[] origin, Neighbours neighbours) {
            this(section.size(), section.key().stride(), new int[(section.size() + 2) * (section.size() + 2)][]);
            SectionKey key = section.key();
            for (int v = -1; v <= size; v++) {
                for (int u = -1; u <= size; u++) {
                    boolean inside = u >= 0 && v >= 0 && u < size && v < size;
                    if (!inside) {
                        runs[(u + 1) + (v + 1) * (size + 2)] =
                                neighbours.runs(key.face(), origin[0] + u * stride, origin[1] + v * stride, key.level());
                    }
                }
            }
        }

        /** The runs of the column (u, v), from -1 to size on either axis; null if unknown. */
        int[] at(int u, int v) {
            if (u < -1 || v < -1 || u > size || v > size) {
                return null;
            }
            return runs[(u + 1) + (v + 1) * (size + 2)];
        }

        void fill(int u, int v, int[] columnRuns) {
            runs[(u + 1) + (v + 1) * (size + 2)] = columnRuns;
        }

        private int[] columnAt(float x, float z) {
            return at(Math.floorDiv((int) Math.floor(x), stride), Math.floorDiv((int) Math.floor(z), stride));
        }

        /** What the block at this point is: zero for air, else {@link #SOLID}, {@link #WATER}... */
        int kindAt(float x, float y, float z) {
            int[] column = columnAt(x, z);
            if (column == null) {
                return 0;
            }
            int cell = (int) Math.floor(y);
            for (int k = 0; k < column.length; k += RUN) {
                if (cell > column[k + 1]) {
                    return 0;
                }
                if (cell >= column[k]) {
                    return column[k + 2];
                }
            }
            return 0;
        }

        /**
         * The sunlight of the block at this point, as the engine propagates it: full under the open sky,
         * one less for every block of water above, and none under ground. A block of ground holds none.
         */
        int sunlightAt(float x, float y, float z) {
            int[] column = columnAt(x, z);
            int cell = (int) Math.floor(y);
            if (column == null || column.length == 0 || cell > column[1]) {
                return FULL_LIGHT;
            }
            if (column[2] == WATER && cell >= column[0]) {
                return Math.max(0, FULL_LIGHT - (column[1] - cell + 1));
            }
            return 0;
        }

        /** The engine's occlusion of a vertex: four blocks eight tenths out along the normal. */
        float occlusion(Vector3fc p, Vector3fc normal) {
            int opaque = 0;
            int billboard = 0;
            for (int i = 0; i < 4; i++) {
                float s = i == 0 || i == 3 ? 0.1f : -0.1f;
                float t = i < 2 ? 0.1f : -0.1f;
                float x;
                float y;
                float z;
                if (normal.x() != 0) {
                    x = p.x() + 0.8f * normal.x();
                    y = p.y() + t;
                    z = p.z() + s;
                } else if (normal.z() != 0) {
                    x = p.x() + s;
                    y = p.y() + t;
                    z = p.z() + 0.8f * normal.z();
                } else {
                    x = p.x() + s;
                    y = p.y() + 0.8f * normal.y();
                    z = p.z() + t;
                }
                int kind = kindAt(x, y, z);
                if (kind == SOLID) {
                    opaque++;
                } else if (kind == WATER) {
                    billboard++;
                }
            }
            return (float) ((OPAQUE_OCCLUSION[opaque] + BILLBOARD_OCCLUSION[billboard]) / 2.0);
        }

        /** The engine's sunlight of a vertex: the mean of the lit ones among its eight samples. */
        float sunlight(Vector3fc p) {
            int sum = 0;
            int lit = 0;
            for (int i = 0; i < LIGHT_SAMPLE_X.length; i++) {
                int sun = sunlightAt(p.x() + LIGHT_SAMPLE_X[i], p.y() + LIGHT_SAMPLE_Y[i], p.z() + LIGHT_SAMPLE_Z[i]);
                if (sun > 0) {
                    sum += sun;
                    lit++;
                }
            }
            return lit == 0 ? 0 : sum / (float) lit / FULL_LIGHT;
        }
    }

    /** White, for materials that carry no tint. */
    public static final Colorc NO_TINT = Color.white;
}
