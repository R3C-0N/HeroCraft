// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

/**
 * A square of columns on one face, each summarised as a short list of vertical segments.
 * <p>
 * A column and not a height, because the ground of this world is full of overhangs — cave mouths,
 * cliffs cut by galleries — and a height field would flatten every one of them. A segment is a run
 * of cells of one material: where it starts, where it ends, what it is, and the light it gets. The
 * light is baked: there is no dynamic light in the distance, which is what makes the distance
 * affordable.
 * <p>
 * Stored flat, one {@code long} per segment and an offset table per column, so that a section is two
 * arrays and nothing else — no object per column, nothing for the collector to walk.
 */
public final class ColumnSection {

    private static final int HEIGHT_BITS = 10;
    private static final int MATERIAL_BITS = 16;
    private static final long HEIGHT_MASK = (1L << HEIGHT_BITS) - 1;
    private static final long MATERIAL_MASK = (1L << MATERIAL_BITS) - 1;
    private static final int TOP_SHIFT = HEIGHT_BITS;
    private static final int MATERIAL_SHIFT = 2 * HEIGHT_BITS;
    private static final int SKY_SHIFT = MATERIAL_SHIFT + MATERIAL_BITS;
    private static final int BLOCK_SHIFT = SKY_SHIFT + 4;
    private static final int LIQUID_SHIFT = BLOCK_SHIFT + 4;

    /** The largest height a segment can carry above the floor of the world. */
    public static final int MAX_HEIGHT = (int) HEIGHT_MASK;

    private final SectionKey key;
    private final int size;
    private final int floorY;
    private final int[] offsets;
    private final long[] segments;

    ColumnSection(SectionKey key, int size, int floorY, int[] offsets, long[] segments) {
        this.key = key;
        this.size = size;
        this.floorY = floorY;
        this.offsets = offsets;
        this.segments = segments;
    }

    public SectionKey key() {
        return key;
    }

    /** Columns along a side. */
    public int size() {
        return size;
    }

    /** How many segments the column at (u, v) holds. */
    public int count(int u, int v) {
        int c = u + v * size;
        return offsets[c + 1] - offsets[c];
    }

    /** The {@code k}-th segment of a column, top first. */
    public long segment(int u, int v, int k) {
        return segments[offsets[u + v * size] + k];
    }

    public int segmentCount() {
        return segments.length;
    }

    /** What the section weighs in memory: its two arrays. */
    public long bytes() {
        return 8L * segments.length + 4L * offsets.length;
    }

    public int bottom(long segment) {
        return floorY + (int) (segment & HEIGHT_MASK);
    }

    public int top(long segment) {
        return floorY + (int) ((segment >>> TOP_SHIFT) & HEIGHT_MASK);
    }

    public static int material(long segment) {
        return (int) ((segment >>> MATERIAL_SHIFT) & MATERIAL_MASK);
    }

    public static int skyLight(long segment) {
        return (int) ((segment >>> SKY_SHIFT) & 0xF);
    }

    public static int blockLight(long segment) {
        return (int) ((segment >>> BLOCK_SHIFT) & 0xF);
    }

    public static boolean isLiquid(long segment) {
        return ((segment >>> LIQUID_SHIFT) & 1) != 0;
    }

    /** The lowest block of a run, above the floor of the world. */
    static int relativeBottom(long segment) {
        return (int) (segment & HEIGHT_MASK);
    }

    /** The highest block of a run, above the floor of the world. */
    static int relativeTop(long segment) {
        return (int) ((segment >>> TOP_SHIFT) & HEIGHT_MASK);
    }

    /** The same run, carried down to the floor of the world. */
    static long downToFloor(long segment) {
        return segment & ~HEIGHT_MASK;
    }

    /**
     * @param bottom the lowest block of the run, above the floor of the world
     * @param top the highest block of the run, inclusive, above the floor of the world
     */
    static long pack(int bottom, int top, int material, int sky, int block, boolean liquid) {
        if (bottom < 0 || top < bottom || top > MAX_HEIGHT) {
            throw new IllegalArgumentException("segment " + bottom + ".." + top + " does not fit");
        }
        if (material < 0 || material > MATERIAL_MASK) {
            throw new IllegalArgumentException("material " + material + " does not fit");
        }
        return bottom
                | ((long) top << TOP_SHIFT)
                | ((long) material << MATERIAL_SHIFT)
                | ((long) (sky & 0xF) << SKY_SHIFT)
                | ((long) (block & 0xF) << BLOCK_SHIFT)
                | ((liquid ? 1L : 0L) << LIQUID_SHIFT);
    }
}
