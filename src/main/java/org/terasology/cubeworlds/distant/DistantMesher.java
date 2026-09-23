// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector2f;
import org.joml.Vector2fc;
import org.joml.Vector3f;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.rendering.primitives.ChunkMeshImpl;
import org.terasology.engine.rendering.primitives.ChunkVertexFlag;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

import java.util.Arrays;

/**
 * Turns a node of columns into a chunk mesh: the top of every run, and the walls a run shows to its
 * four neighbours.
 * <p>
 * The mesh is an ordinary {@link ChunkMesh}, in the chunk's own vertex format, and it is drawn by the
 * chunk shader. That is the whole point: a distant vertex is placed on the sphere by the very
 * function that places a vertex of the terrain, from the same kind of input — a position on the net,
 * relative to an origin on the net — so the two cannot disagree about where the world is.
 * <p>
 * No triangle crosses a seam. A node lies on one face, and a wall is always built on the node's own
 * side of its edge, down to what the neighbour shows; the neighbour is read through
 * {@link Neighbours}, which folds across the seam. What lies across is asked, never computed from a
 * difference of coordinates.
 */
public final class DistantMesher {

    /** The runs a neighbouring column shows, top first, as {bottom, top} pairs of block heights. */
    @FunctionalInterface
    public interface Neighbours {
        /**
         * @param netX a column of the net, possibly past the edge of the node's face
         * @param netZ likewise
         * @return the opaque runs of the column there, or null if nothing is known about it yet
         */
        int[] opaqueRuns(int face, int netX, int netZ, int level);
    }

    /** How a material looks: where its faces are in the atlas, and their tint. */
    public interface Looks {
        Vector2fc topTile(int material);

        Vector2fc sideTile(int material);

        Colorc topTint(int material);

        Colorc sideTint(int material);

        /** Grass carries its tint in a mask on its sides, and the shader wants to be told. */
        boolean isGrass(int material);
    }

    private static final Vector3f UP = new Vector3f(0, 1, 0);
    private static final Vector3f[] SIDE_NORMALS = {
            new Vector3f(-1, 0, 0), new Vector3f(1, 0, 0), new Vector3f(0, 0, -1), new Vector3f(0, 0, 1)};
    private static final int[][] SIDE_STEP = {{-1, 0}, {1, 0}, {0, -1}, {0, 1}};
    private static final int SKIRT_COLUMNS = 4;

    private final Looks looks;

    public DistantMesher(Looks looks) {
        this.looks = looks;
    }

    /**
     * @param section the node
     * @param origin the net column of its first column, as {x, z}
     * @param neighbours what lies around it
     * @return the mesh, with vertices relative to {@code origin} at height zero
     */
    public ChunkMesh mesh(ColumnSection section, int[] origin, Neighbours neighbours) {
        ChunkMeshImpl mesh = new ChunkMeshImpl();
        ChunkMesh.VertexElements out = mesh.getVertexElements(ChunkMesh.RenderType.OPAQUE);
        int size = section.size();
        int stride = section.key().stride();
        int face = section.key().face();
        int level = section.key().level();
        Vector3f a = new Vector3f();
        Vector3f b = new Vector3f();
        Vector3f c = new Vector3f();
        Vector3f d = new Vector3f();
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                float x0 = u * stride;
                float z0 = v * stride;
                float x1 = x0 + stride;
                float z1 = z0 + stride;
                int count = section.count(u, v);
                for (int k = 0; k < count; k++) {
                    long seg = section.segment(u, v, k);
                    int material = ColumnSection.material(seg);
                    float sky = ColumnSection.skyLight(seg) / 15f;
                    float block = ColumnSection.blockLight(seg) / 15f;
                    float top = section.top(seg) + 1;
                    // A top is drawn unless the run above sits right on it.
                    boolean covered = k > 0 && section.bottom(section.segment(u, v, k - 1)) == section.top(seg) + 1;
                    if (!covered) {
                        quad(out, a.set(x0, top, z0), b.set(x0, top, z1), c.set(x1, top, z1), d.set(x1, top, z0),
                                UP, looks.topTile(material), looks.topTint(material), ChunkVertexFlag.NORMAL,
                                sky, block);
                    }
                    if (ColumnSection.isLiquid(seg)) {
                        continue;
                    }
                    for (int side = 0; side < 4; side++) {
                        int nu = u + SIDE_STEP[side][0];
                        int nv = v + SIDE_STEP[side][1];
                        int[] other = nu >= 0 && nv >= 0 && nu < size && nv < size
                                ? opaqueRuns(section, nu, nv)
                                : neighbours.opaqueRuns(face, origin[0] + (nu * stride), origin[1] + (nv * stride), level);
                        if (other == null) {
                            // Unknown ground next door: a skirt, four columns deep, rather than a wall
                            // to the floor of the world that the neighbour will cover a moment later.
                            // It hides the crack between two nodes whose heights disagree.
                            emitWall(out, side, x0, z0, x1, z1, Math.max(section.bottom(seg),
                                    section.top(seg) - SKIRT_COLUMNS * stride), section.top(seg), material,
                                    sky, block, a, b, c, d);
                            continue;
                        }
                        wall(out, side, x0, z0, x1, z1, section.bottom(seg), section.top(seg), other, material,
                                sky, block, a, b, c, d);
                    }
                }
            }
        }
        return mesh;
    }

    /** The parts of a run no neighbouring run hides, as walls facing that neighbour. */
    private void wall(ChunkMesh.VertexElements out, int side, float x0, float z0, float x1, float z1,
                      int bottom, int top, int[] other, int material, float sky, float block,
                      Vector3f a, Vector3f b, Vector3f c, Vector3f d) {
        int from = top;
        // Walk down the run, skipping what the neighbour covers.
        for (int k = 0; k + 1 < other.length && from >= bottom; k += 2) {
            int otherBottom = other[k];
            int otherTop = other[k + 1];
            if (otherTop < bottom) {
                break;
            }
            if (otherTop < from) {
                emitWall(out, side, x0, z0, x1, z1, Math.max(otherTop + 1, bottom), from, material, sky, block,
                        a, b, c, d);
            }
            from = Math.min(from, otherBottom - 1);
        }
        if (from >= bottom) {
            emitWall(out, side, x0, z0, x1, z1, bottom, from, material, sky, block, a, b, c, d);
        }
    }

    private void emitWall(ChunkMesh.VertexElements out, int side, float x0, float z0, float x1, float z1,
                          int low, int high, int material, float sky, float block,
                          Vector3f a, Vector3f b, Vector3f c, Vector3f d) {
        float y0 = low;
        float y1 = high + 1;
        ChunkVertexFlag flag = looks.isGrass(material) ? ChunkVertexFlag.COLOR_MASK : ChunkVertexFlag.NORMAL;
        Vector2fc tile = looks.sideTile(material);
        Colorc tint = looks.sideTint(material);
        switch (side) {
            case 0:
                quad(out, a.set(x0, y1, z0), b.set(x0, y0, z0), c.set(x0, y0, z1), d.set(x0, y1, z1),
                        SIDE_NORMALS[0], tile, tint, flag, sky, block);
                break;
            case 1:
                quad(out, a.set(x1, y1, z1), b.set(x1, y0, z1), c.set(x1, y0, z0), d.set(x1, y1, z0),
                        SIDE_NORMALS[1], tile, tint, flag, sky, block);
                break;
            case 2:
                quad(out, a.set(x1, y1, z0), b.set(x1, y0, z0), c.set(x0, y0, z0), d.set(x0, y1, z0),
                        SIDE_NORMALS[2], tile, tint, flag, sky, block);
                break;
            default:
                quad(out, a.set(x0, y1, z1), b.set(x0, y0, z1), c.set(x1, y0, z1), d.set(x1, y1, z1),
                        SIDE_NORMALS[3], tile, tint, flag, sky, block);
                break;
        }
    }

    /** Opaque runs of a column of the node, as {bottom, top} pairs, top first. */
    static int[] opaqueRuns(ColumnSection section, int u, int v) {
        int count = section.count(u, v);
        int[] runs = new int[2 * count];
        int n = 0;
        for (int k = 0; k < count; k++) {
            long seg = section.segment(u, v, k);
            if (ColumnSection.isLiquid(seg)) {
                continue;
            }
            runs[n++] = section.bottom(seg);
            runs[n++] = section.top(seg);
        }
        return n == runs.length ? runs : Arrays.copyOf(runs, n);
    }

    /**
     * One quad, counter-clockwise seen from where its normal points. The uv is the corner of the
     * tile: the shader repeats the tile once per block of the world, so a column sixteen blocks wide
     * still shows grass the size of grass.
     */
    private static void quad(ChunkMesh.VertexElements out, Vector3f a, Vector3f b, Vector3f c, Vector3f d,
                             Vector3f normal, Vector2fc tile, Colorc tint, ChunkVertexFlag flag,
                             float sky, float block) {
        int base = out.vertexCount;
        Vector2f uv = new Vector2f(tile);
        for (Vector3f p : new Vector3f[]{a, b, c, d}) {
            out.position.put(p);
            out.normals.put(normal);
            out.uv0.put(uv);
            out.flags.put((byte) flag.getValue());
            out.frames.put((byte) 0);
            out.waterDepth.put((byte) 0);
            out.color.put(tint);
            out.sunlight.put(sky);
            out.blockLight.put(block);
            out.warmth.put((byte) (block > 0 ? 127 : 0));
            out.ambientOcclusion.put(1.0f);
        }
        out.vertexCount += 4;
        out.indices.put(base);
        out.indices.put(base + 1);
        out.indices.put(base + 2);
        out.indices.put(base + 2);
        out.indices.put(base + 3);
        out.indices.put(base);
    }

    /** White, for materials that carry no tint. */
    public static final Colorc NO_TINT = Color.white;
}
