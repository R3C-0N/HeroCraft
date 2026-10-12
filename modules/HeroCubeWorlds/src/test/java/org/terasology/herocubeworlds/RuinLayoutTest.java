// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.RuinCulture;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.herocubeworlds.generator.RuinShape;
import org.terasology.herocubeworlds.topology.CubePattern;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Where the ruins may stand and what they look like, with no world generated: the lattice and the
 * plan are pure functions, and these are the properties every chunk relies on to agree.
 */
class RuinLayoutTest {

    private static final int EDGE = CubeGeometry.createDefault().faceBlocks();

    @Test
    void everyLiveCellHasOneSiteWellInsideItsFace() {
        RuinLattice lattice = new RuinLattice(0xC0FFEE, EDGE);
        List<RuinLattice.Site> sites = lattice.sitesNear(0, 0,
                EDGE * CubePattern.PATTERN_SIDE - 1, EDGE * CubePattern.PATTERN_SIDE - 1, 0);
        int perEdge = EDGE / RuinLattice.CELL;
        assertEquals(CubePattern.FACES * perEdge * perEdge, sites.size(),
                "one candidate per cell of every face, and none on the dead slots");
        for (RuinLattice.Site site : sites) {
            int u = Math.floorMod(site.x(), EDGE);
            int v = Math.floorMod(site.z(), EDGE);
            int margin = Math.min(Math.min(u, EDGE - 1 - u), Math.min(v, EDGE - 1 - v));
            assertTrue(margin > RuinShape.RADIUS, "a ruin would straddle a seam at " + site.x() + "," + site.z());
            assertEquals(CubePattern.faceOf(site.x(), site.z(), EDGE), site.face());
        }
    }

    @Test
    void theSitesAreAFactOfTheSeed() {
        RuinLattice a = new RuinLattice(42, EDGE);
        RuinLattice b = new RuinLattice(42, EDGE);
        RuinLattice c = new RuinLattice(43, EDGE);
        int cellX = EDGE / RuinLattice.CELL + 2;
        int cellZ = 2;
        assertEquals(a.site(cellX, cellZ).x(), b.site(cellX, cellZ).x());
        assertEquals(a.site(cellX, cellZ).seed(), b.site(cellX, cellZ).seed());
        assertNotEquals(a.site(cellX, cellZ).seed(), c.site(cellX, cellZ).seed());
    }

    @Test
    void theAnchorStandsOnTheAltarAndTheGateIsOpen() {
        for (long seed = 0; seed < 64; seed++) {
            assertEquals(RuinShape.Part.ALTAR, RuinShape.part(0, 1, 0, seed));
            assertEquals(RuinShape.Part.ANCHOR, RuinShape.part(0, RuinShape.anchorHeight(), 0, seed));
            int r = RuinShape.RADIUS;
            int[][] gates = {{r, 0}, {-r, 0}, {0, r}, {0, -r}};
            int[] gate = gates[RuinShape.gateSide(seed)];
            // From the gap in the wall to the foot of the altar, nothing to climb over.
            for (int step = 0; step < r; step++) {
                int dx = Integer.signum(gate[0]) * (r - step);
                int dz = Integer.signum(gate[1]) * (r - step);
                for (int dy = 1; dy <= 2; dy++) {
                    assertEquals(RuinShape.Part.CLEAR, RuinShape.part(dx, dy, dz, seed),
                            "the way in is blocked at " + dx + "," + dy + "," + dz + " for seed " + seed);
                }
            }
        }
    }

    @Test
    void thePlanStaysInsideItsBox() {
        for (long seed = 0; seed < 16; seed++) {
            for (int dy = -RuinShape.FOUNDATION_DEPTH - 2; dy <= RuinShape.CLEAR_HEIGHT + 2; dy++) {
                for (int d = -RuinShape.RADIUS - 2; d <= RuinShape.RADIUS + 2; d++) {
                    boolean inside = Math.abs(d) <= RuinShape.RADIUS
                            && dy >= -RuinShape.FOUNDATION_DEPTH && dy <= RuinShape.CLEAR_HEIGHT;
                    assertEquals(inside, RuinShape.part(d, dy, 0, seed) != RuinShape.Part.NONE);
                }
            }
        }
    }

    @Test
    void everyCultureHasABlockForEveryPart() {
        for (RuinCulture culture : RuinCulture.values()) {
            for (RuinShape.Part part : RuinShape.Part.values()) {
                if (part != RuinShape.Part.NONE && part != RuinShape.Part.CLEAR) {
                    assertNotNull(culture.blockOf(part));
                }
            }
        }
    }
}
