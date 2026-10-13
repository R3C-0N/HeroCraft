// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The band of finest columns round the edge of the chunks. */
class DistantSelectionTest {

    private static final double REACH = 2000.0;
    private static final double FRONTIER = 208.0;
    private static final double FROM = FRONTIER - 32.0;

    private final CubeGeometry geometry = CubeGeometry.createDefault();
    private final DistantSelection selection = new DistantSelection(geometry, 1);
    private final int[] eye = geometry.faceCentre(0);
    private final double to = FRONTIER * Math.sqrt(2.0) + SectionKey.columns(geometry);

    @Test
    void withoutBandNothingAtTheFrontierIsFinest() {
        for (SectionKey key : selection.select(eye[0], eye[1], REACH)) {
            if (straddles(key, FRONTIER)) {
                assertTrue(key.level() >= 1, "a node across the frontier at level " + key.level());
            }
        }
    }

    @Test
    void theFrontierIsDrawnAtOneColumnPerBlock() {
        List<SectionKey> chosen = selection.select(eye[0], eye[1], REACH, FROM, to);
        int across = 0;
        for (SectionKey key : chosen) {
            if (straddles(key, FRONTIER)) {
                assertEquals(0, key.level(), "a node across the frontier at level " + key.level());
                across++;
            }
        }
        assertTrue(across > 0, "no node across the frontier");
    }

    /** A node is split whole: a finest node is there because its parent reached into the band, never otherwise. */
    @Test
    void finestNodesComeOnlyFromTheBand() {
        int finest = 0;
        for (SectionKey key : selection.select(eye[0], eye[1], REACH, FROM, to)) {
            if (key.level() == 0) {
                SectionKey parent = new SectionKey(key.face(), 1, key.i() / 2, key.j() / 2);
                assertTrue(selection.distanceTo(parent, eye[0], eye[1]) < to
                        && selection.farthest(parent, eye[0], eye[1]) > FROM, "a finest node away from the band: " + key);
                finest++;
            }
        }
        assertTrue(finest > 0, "no finest node at all");
    }

    private boolean straddles(SectionKey key, double radius) {
        return selection.distanceTo(key, eye[0], eye[1]) < radius && selection.farthest(key, eye[0], eye[1]) > radius;
    }
}
