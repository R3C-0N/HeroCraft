// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.terasology.herocubeworlds.generator.CubeGeometry;

import java.util.ArrayList;
import java.util.List;

/**
 * Which nodes of the six quadtrees stand for the world, seen from one eye.
 * <p>
 * Each tree is walked from its root, and a node is split while it is too close for its own
 * resolution: a column must never look wider than {@link #COLUMNS_PER_DISTANCE} of the distance to
 * it. The distance is the world's — {@link CubeGeometry#angle}, from the eye's column to the node's
 * nearest reach — never a difference of coordinates, which across a seam would call a neighbour a
 * whole face away and hand it the coarsest level on the planet.
 * <p>
 * Where the chunks end, that rule is not enough: a two-block column there stands next to the blocks
 * it replaces, and its steps, its shifted shores and its puddles read as another terrain. So a node
 * that reaches into a band round the edge of the chunks is split down to the finest level, whatever
 * its distance — level 0, one column per block, below the finest level drawn anywhere else — and the
 * coarsening starts past the band.
 * <p>
 * Pure: no engine, no chunk, no renderer. A census can ask it where the levels fall.
 */
public final class DistantSelection {

    /**
     * A node of span {@code w} is kept whole once it is at least this many spans away. Two means a
     * column covers at most a fiftieth of its distance — eight pixels of a 1280 wide view at
     * ninety degrees — and it keeps the number of nodes in view to a few dozen.
     */
    public static final double COLUMNS_PER_DISTANCE = 2.0;

    private final CubeGeometry geometry;
    private final int finestLevel;

    /** @param finestLevel no node finer than this is ever chosen */
    public DistantSelection(CubeGeometry geometry, int finestLevel) {
        this.geometry = geometry;
        this.finestLevel = finestLevel;
    }

    /**
     * @param eyeX the eye's column on the net
     * @param eyeZ likewise
     * @param reach nothing further than this, in blocks along the surface, is kept
     */
    public List<SectionKey> select(double eyeX, double eyeZ, double reach) {
        return select(eyeX, eyeZ, reach, 0.0, 0.0);
    }

    /**
     * @param eyeX the eye's column on the net
     * @param eyeZ likewise
     * @param reach nothing further than this, in blocks along the surface, is kept
     * @param sharpFrom where the band drawn at level 0 begins, in blocks along the surface
     * @param sharpTo where it ends; no band when it is not past {@code sharpFrom}
     */
    public List<SectionKey> select(double eyeX, double eyeZ, double reach, double sharpFrom, double sharpTo) {
        List<SectionKey> chosen = new ArrayList<>();
        for (int face = 0; face < 6; face++) {
            walk(new SectionKey(face, SectionKey.TOP_LEVEL, 0, 0), eyeX, eyeZ, reach, sharpFrom, sharpTo, chosen);
        }
        return chosen;
    }

    private void walk(SectionKey key, double eyeX, double eyeZ, double reach, double sharpFrom, double sharpTo,
                      List<SectionKey> chosen) {
        double distance = distanceTo(key, eyeX, eyeZ);
        if (distance > reach) {
            return;
        }
        double span = key.span(geometry);
        boolean inBand = distance < sharpTo && farthest(key, eyeX, eyeZ) > sharpFrom;
        if (inBand ? key.level() > 0 : key.level() > finestLevel && distance < COLUMNS_PER_DISTANCE * span) {
            for (SectionKey child : key.children()) {
                walk(child, eyeX, eyeZ, reach, sharpFrom, sharpTo, chosen);
            }
            return;
        }
        chosen.add(key);
    }

    /** From the eye to the furthest part of a node, in blocks along the surface: the other side of distanceTo. */
    double farthest(SectionKey key, double eyeX, double eyeZ) {
        int[] origin = key.netOrigin(geometry);
        double half = key.span(geometry) / 2.0;
        double arc = geometry.arc(eyeX, eyeZ, origin[0] + half, origin[1] + half);
        return arc + half * Math.sqrt(2.0) * CubeGeometry.MAX_SCALE_FACTOR;
    }

    /**
     * From the eye to the nearest part of a node, in blocks along the surface: to its centre, less
     * half its diagonal at the largest a block gets. Never below zero, which is where the eye is in
     * the node.
     */
    public double distanceTo(SectionKey key, double eyeX, double eyeZ) {
        int[] origin = key.netOrigin(geometry);
        double half = key.span(geometry) / 2.0;
        double arc = geometry.arc(eyeX, eyeZ, origin[0] + half, origin[1] + half);
        return Math.max(0.0, arc - half * Math.sqrt(2.0) * CubeGeometry.MAX_SCALE_FACTOR);
    }
}
