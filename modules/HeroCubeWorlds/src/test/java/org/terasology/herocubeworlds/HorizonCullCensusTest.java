// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3f;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.rendering.SphericalChunkVisibilityPolicy;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.joml.geom.AABBf;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Which loaded chunks the horizon test throws away, against which ones the planet really hides.
 * <p>
 * The truth is geometric: a chunk is hidden when every line of sight from the eye to its top meets
 * the floor of the world first. The floor is the only thing the planet is guaranteed to be made of,
 * so a test that culls anything the floor does not hide is culling on a guess. That guess is what
 * left the underground undrawn — the ray aimed at the rock found it, and the screen showed the sky.
 * <p>
 * The eye goes where the rule is most likely to break: the middle of a face, the edge of a cut seam,
 * and one of the eight corners, from the floor up to the ceiling.
 */
class HorizonCullCensusTest {

    /** The loaded box around the eye at the default view distance: 13 x 7 x 13 chunks. */
    private static final int HALF_X = 6;
    private static final int HALF_Y = 3;
    private static final int HALF_Z = 6;

    private static final float[] EYE_HEIGHTS = {-180f, -150f, -100f, -40f, 0f, 20f, 33.6f, 40f, 60f, 100f, 200f, 300f};

    private static CubeGeometry geometry;

    @BeforeAll
    static void buildGeometry() {
        geometry = CubeGeometry.createDefault();
    }

    private record Place(String name, float x, float z) {
    }

    private static List<Place> places() {
        int face = geometry.faceBlocks();
        List<Place> places = new ArrayList<>();
        int[] centre = geometry.faceCentre(1);
        places.add(new Place("milieu de face", centre[0] + 0.5f, centre[1] + 0.5f));
        // The +Z face sits in slot (1, 2); the slot to its left is dead, so its left edge is a cut
        // seam, folded onto the top edge of +X.
        places.add(new Place("couture coupee", face + 20.5f, 2 * face + face / 2 + 0.5f));
        // The corner of +Y shared with +X and +Z.
        places.add(new Place("coin", face + 15.5f, 2 * face - 15.5f));
        return places;
    }

    /** One census line: chunks thrown away by a rule, and how many of those were in fact visible. */
    private static final class Count {
        int culled;
        int wronglyCulled;
    }

    @Test
    void theHorizonNeverCullsWhatTheFloorDoesNotHide() {
        assertTrue(maxScaleFactor() <= CubeGeometry.MAX_SCALE_FACTOR + 1e-9,
                "a block is larger than MAX_SCALE_FACTOR, the occluder is no longer below the floor");

        System.out.printf("%-16s %8s %7s | %-17s | %-17s%n", "lieu", "oeil Y", "chunks",
                "ancienne regle", "nouvelle regle");
        int totalNewWrong = 0;
        int totalOldWrong = 0;
        int undergroundOldWrong = 0;
        for (Place place : places()) {
            for (float eyeY : EYE_HEIGHTS) {
                Vector3f eye = new Vector3f(place.x(), eyeY, place.z());
                Count old = new Count();
                Count current = new Count();
                int chunks = 0;
                int cx = Math.floorDiv((int) Math.floor(eye.x), Chunks.SIZE_X);
                int cy = Math.floorDiv((int) Math.floor(eye.y), Chunks.SIZE_Y);
                int cz = Math.floorDiv((int) Math.floor(eye.z), Chunks.SIZE_Z);
                for (int x = cx - HALF_X; x <= cx + HALF_X; x++) {
                    for (int y = cy - HALF_Y; y <= cy + HALF_Y; y++) {
                        if (y * Chunks.SIZE_Y < geometry.floorY() || y * Chunks.SIZE_Y > geometry.ceilingY()) {
                            continue;
                        }
                        for (int z = cz - HALF_Z; z <= cz + HALF_Z; z++) {
                            AABBf bounds = chunkBounds(x, y, z);
                            double midX = (bounds.minX + bounds.maxX) * 0.5;
                            double midZ = (bounds.minZ + bounds.maxZ) * 0.5;
                            if (Double.isNaN(geometry.renderRadius(midX, 0, midZ))) {
                                continue; // a dead slot: nothing is ever there to be drawn
                            }
                            chunks++;
                            boolean visible = anyTopPointVisible(eye, bounds);
                            tally(old, oldRule(eye, bounds), visible);
                            tally(current, SphericalChunkVisibilityPolicy.withinHorizon(geometry, eye, bounds), visible);
                        }
                    }
                }
                System.out.printf("%-16s %8.1f %7d | %6d rejetes %4d a tort | %6d rejetes %4d a tort%n",
                        place.name(), eyeY, chunks, old.culled, old.wronglyCulled,
                        current.culled, current.wronglyCulled);
                totalNewWrong += current.wronglyCulled;
                totalOldWrong += old.wronglyCulled;
                if (eyeY < geometry.seaLevel()) {
                    undergroundOldWrong += old.wronglyCulled;
                }
            }
        }
        System.out.printf("a tort : ancienne %d, nouvelle %d%n", totalOldWrong, totalNewWrong);
        assertEquals(0, totalNewWrong, "the horizon culls chunks the floor does not hide");
        // Kept as an assertion so that the census keeps saying what it found: if this ever reads
        // zero, the old rule was not the defect and the story in the docs is wrong.
        assertTrue(undergroundOldWrong > 0, "the old rule was expected to cull visible chunks underground");
    }

    private static void tally(Count count, boolean kept, boolean visible) {
        if (!kept) {
            count.culled++;
            if (visible) {
                count.wronglyCulled++;
            }
        }
    }

    private static AABBf chunkBounds(int x, int y, int z) {
        // As BlockRegion.getBounds gives it to the renderer: half a block of margin on each side.
        return new AABBf(
                x * Chunks.SIZE_X - 0.5f, y * Chunks.SIZE_Y - 0.5f, z * Chunks.SIZE_Z - 0.5f,
                (x + 1) * Chunks.SIZE_X - 0.5f, (y + 1) * Chunks.SIZE_Y - 0.5f, (z + 1) * Chunks.SIZE_Z - 0.5f);
    }

    /**
     * Whether some point of the chunk's top face can be seen without the line of sight passing
     * through the floor. The top is what shows first; the corners are where it shows from furthest.
     */
    private static boolean anyTopPointVisible(Vector3f eye, AABBf bounds) {
        double[] e = absolute(eye.x, eye.y, eye.z);
        float inset = 1.0f;
        float[] xs = {bounds.minX + inset, (bounds.minX + bounds.maxX) * 0.5f, bounds.maxX - inset};
        float[] zs = {bounds.minZ + inset, (bounds.minZ + bounds.maxZ) * 0.5f, bounds.maxZ - inset};
        for (float x : xs) {
            for (float z : zs) {
                double[] p = absolute(x, bounds.maxY, z);
                if (p != null && !segmentMeetsSphere(e, p, geometry.occluderRadius())) {
                    return true;
                }
            }
        }
        return false;
    }

    /** Where the shader draws a point, from the centre of the planet; null off the net. */
    private static double[] absolute(double x, double y, double z) {
        double[] dir = new double[3];
        if (!geometry.columnDirection(x, z, dir)) {
            return null;
        }
        double r = geometry.renderRadius(x, y, z);
        return new double[]{dir[0] * r, dir[1] * r, dir[2] * r};
    }

    private static boolean segmentMeetsSphere(double[] a, double[] b, double radius) {
        double[] d = {b[0] - a[0], b[1] - a[1], b[2] - a[2]};
        double dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        double t = dd == 0 ? 0 : -(a[0] * d[0] + a[1] * d[1] + a[2] * d[2]) / dd;
        t = Math.max(0.0, Math.min(1.0, t));
        double px = a[0] + t * d[0];
        double py = a[1] + t * d[1];
        double pz = a[2] + t * d[2];
        return px * px + py * py + pz * pz < radius * radius;
    }

    /**
     * The rule this census replaced, copied here to keep its failure measurable: heights above sea
     * level with nothing below counted, and a straight line from the camera taken for a distance.
     */
    private static boolean oldRule(Vector3f eye, AABBf bounds) {
        double eyeReach = geometry.horizon(Math.max(0.0f, eye.y - geometry.seaLevel()));
        double chunkReach = geometry.horizon(Math.max(0.0f, bounds.maxY - geometry.seaLevel()));
        double[] e = absolute(eye.x, eye.y, eye.z);
        double[] c = absolute((bounds.minX + bounds.maxX) * 0.5, (bounds.minY + bounds.maxY) * 0.5,
                (bounds.minZ + bounds.maxZ) * 0.5);
        double dx = c[0] - e[0];
        double dy = c[1] - e[1];
        double dz = c[2] - e[2];
        double chord = Math.sqrt(dx * dx + dy * dy + dz * dz);
        double radius = geometry.radius();
        double separation = 2.0 * radius * Math.asin(Math.min(1.0, chord / (2.0 * radius)));
        return separation <= eyeReach + chunkReach + Chunks.SIZE_X;
    }

    private static double maxScaleFactor() {
        double max = 0.0;
        for (int i = 0; i <= 256; i++) {
            for (int j = 0; j <= 256; j++) {
                max = Math.max(max, geometry.table().lambda(-1.0 + i / 128.0, -1.0 + j / 128.0));
            }
        }
        return max;
    }
}
