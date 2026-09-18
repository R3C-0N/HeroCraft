// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facetProviders;

import org.joml.Vector3ic;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereFieldLattice;
import org.terasology.cubeworlds.generator.facets.CaveFacet;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockAreac;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.ScalableFacetProvider;
import org.terasology.engine.world.generation.Updates;
import org.terasology.engine.world.generation.facets.DensityFacet;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generation.facets.SeaLevelFacet;
import org.terasology.engine.world.generation.facets.SurfacesFacet;

/**
 * Takes the caves out of the rock.
 * <p>
 * <b>A tunnel is an intersection, not a ridge.</b> In three dimensions the set where one scalar
 * field is near zero is a <em>sheet</em>, because a level set of a scalar function is a surface; a
 * world carved from one would be full of enormous flat fissures. A tunnel is a <em>curve</em>, so it
 * takes two independent fields: the set where both are near zero at once, thickened into a tube.
 * Hence {@code n1² + n2² < t²}, squared to keep a square root out of the inner loop.
 * <p>
 * <b>The threshold is computed, not tuned.</b> For two independent fields of width sigma the
 * distance of the pair from the origin is Rayleigh distributed, so a target fraction of the rock
 * fixes the threshold outright: {@code t = sigma * sqrt(-2 ln(1 - phi))}. Sigma is measured, by
 * {@code CaveFieldCalibrationTest}, because it belongs to the engine's noise and not to us.
 * <p>
 * <b>One octave for the tunnels.</b> The weights of a Brownian noise fall as roughly one half per
 * octave, so a fourth octave still carries about an eighth of sigma — and the threshold here is
 * about a sixth of sigma. An octave of that size does not decorate the tube, it pinches it, and the
 * network comes apart into a string of sealed pockets. That failure is invisible in a screenshot
 * and invisible in a volume census; only {@code CaveConnectivityTest} sees it.
 * <p>
 * <b>The depth profile multiplies the radius, never the threshold's meaning.</b> Moving a threshold
 * makes a tunnel stop dead, leaving a section cut clean across; scaling the radius closes it to a
 * point. Same arithmetic, different shape at the end of the gallery.
 * <p>
 * <b>Implements {@link ScalableFacetProvider} and must.</b> {@code WorldBuilder.addProviderChain}
 * filters providers by that interface when the chain is scalable, and drops the others without a
 * word: the facet would simply never be produced.
 */
@Requires({@Facet(ElevationFacet.class), @Facet(SeaLevelFacet.class)})
@Updates({@Facet(DensityFacet.class), @Facet(SurfacesFacet.class)})
@Produces(CaveFacet.class)
public class CubeCaveProvider implements ScalableFacetProvider {

    /** Spacing of the noise lattice, in blocks. Must divide a face edge. */
    private static final int LATTICE_STEP = 4;

    /**
     * Widths of the fields, measured by {@code CaveFieldCalibrationTest} on the engine's noise.
     * They belong to the shape of that noise and to nothing here, so they are checked rather than
     * trusted: retune the engine's simplex and this generator carves either nothing or everything.
     */
    private static final double SIGMA_ONE_OCTAVE = 0.4255;
    private static final double SIGMA_THREE_OCTAVES = 0.2736;

    /**
     * The two systems of galleries. Sizes are in blocks and not counts around the sphere, unlike
     * every field that describes a region: a gallery is a thing a player walks down, and a player
     * is 1.6 blocks tall whatever the planet measures. Tie the bore to the radius and a larger
     * world gets seventeen-block corridors.
     * <p>
     * The vertical size is the smaller of the pair on purpose — see
     * {@code SphereSampler.readAt}: it is what lays the galleries over on their side.
     */
    private static final double MAIN_HORIZONTAL = 64.0;
    private static final double MAIN_VERTICAL = 42.0;
    private static final double MAIN_FRACTION = 0.014;

    /**
     * The minor system is squashed less than the main one, and carves a little more. Its first
     * numbers — 30 blocks vertical at one percent — gave galleries 1.82 blocks high against a
     * character of 1.6, and the connectivity census read it straight away: barely half of the
     * carved cells had a carved cell above them, so half of what had been dug was a crawl.
     */
    private static final double MINOR_HORIZONTAL = 40.0;
    private static final double MINOR_VERTICAL = 34.0;
    private static final double MINOR_FRACTION = 0.012;

    /**
     * Rooms are a swelling of the galleries rather than a system of their own. A separate field of
     * blobs would open caverns with no way in; a wider bore cannot, being the same tunnel.
     */
    private static final double ROOM_HORIZONTAL = 110.0;
    private static final double ROOM_VERTICAL = 50.0;
    private static final double ROOM_LEVEL = 2.0 * SIGMA_THREE_OCTAVES;
    private static final double ROOM_PEAK = 2.8 * SIGMA_THREE_OCTAVES;
    private static final double ROOM_SWELL = 3.0;

    /**
     * Where the caves are allowed to break the surface. A constant depth would either open every
     * tunnel that runs shallow, shredding the ground, or open none at all: the permission has to be
     * a field in its own right, patchy at the size of a cave mouth.
     */
    private static final double ENTRANCE_SIZE = 120.0;
    private static final double ENTRANCE_LEVEL = 0.85;
    private static final double ENTRANCE_SOFT = 0.10;

    /** Rock left over a gallery where no entrance is permitted, and the length of the fade. */
    private static final double ROOF_COVER = 6.0;
    private static final double ROOF_FADE = 16.0;

    /**
     * Rock left under the sea floor, and it is exact rather than hopeful: this chain has no density
     * noise in it, so the density is exactly {@code elevation - y} and a ten block seal is ten
     * blocks. A hole in the sea floor would be a dry shaft under the ocean.
     */
    private static final double SEA_FLOOR_SEAL = 10.0;

    /** Solid rock kept over the mantle, and the fade above it. */
    private static final int FLOOR_MARGIN = 16;
    private static final double FLOOR_FADE = 16.0;

    /** How much wider the galleries run at depth, and the depth at which that is reached. */
    private static final double DEEP_GAIN = 0.20;
    private static final double DEEP_AT = 150.0;

    /** What a carved cell's density becomes. Assigned and not subtracted, so it is idempotent. */
    private static final float CARVED = -1f;

    private final CubeGeometry geometry;
    private Noise mainOne;
    private Noise mainTwo;
    private Noise minorOne;
    private Noise minorTwo;
    private Noise rooms;
    private Noise entrances;

    public CubeCaveProvider(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void setSeed(long seed) {
        // Each field takes its own stream, derived by an independent constant. Deriving them by
        // adding to one seed would correlate them, and two correlated fields do not intersect in a
        // curve — they fall onto each other and the tunnels degenerate back into sheets.
        mainOne = new BrownianNoise(new SimplexNoise(seed ^ 0x3d5c1b77L), 1);
        mainTwo = new BrownianNoise(new SimplexNoise(seed ^ 0x6a2f90e3L), 1);
        minorOne = new BrownianNoise(new SimplexNoise(seed ^ 0x11b4d80fL), 1);
        minorTwo = new BrownianNoise(new SimplexNoise(seed ^ 0x74e6a259L), 1);
        rooms = new BrownianNoise(new SimplexNoise(seed ^ 0x2c871f45L), 3);
        entrances = new BrownianNoise(new SimplexNoise(seed ^ 0x59a3e6cbL), 2);
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        DensityFacet density = region.getRegionFacet(DensityFacet.class);
        CaveFacet caves = new CaveFacet(region.getRegion(), region.getBorderForFacet(DensityFacet.class));

        // No caves in the distance. A level-of-detail cell stands for several blocks, so a gallery
        // six blocks across is below the resolution and would come out as a scatter of single-cell
        // holes in the far terrain. Skipping it also means the whole cost of this provider is paid
        // only where one can actually walk into the result.
        if (scale > 1f) {
            region.setRegionFacet(CaveFacet.class, caves);
            return;
        }

        int face = geometry.faceOfRegion(region.getRegion(), scale);
        if (face == CubePattern.NO_FACE) {
            region.setRegionFacet(CaveFacet.class, caves);
            return;
        }

        float[] densityValues = density.getInternal();
        boolean[] carved = caves.getInternal();
        if (densityValues.length != carved.length) {
            throw new IllegalStateException("the cave facet was built on a different region from "
                    + "the density it carves: " + carved.length + " against " + densityValues.length);
        }
        if (!hasRock(densityValues)) {
            // Open sky or open sea, which is more than half of everything generated. Leaving before
            // the lattice is built is what keeps that half free.
            region.setRegionFacet(CaveFacet.class, caves);
            return;
        }

        SphereFieldLattice lattice = new SphereFieldLattice(geometry, LATTICE_STEP);
        int fieldMainOne = lattice.addVolumeField(mainOne, MAIN_HORIZONTAL, MAIN_VERTICAL);
        int fieldMainTwo = lattice.addVolumeField(mainTwo, MAIN_HORIZONTAL, MAIN_VERTICAL);
        int fieldMinorOne = lattice.addVolumeField(minorOne, MINOR_HORIZONTAL, MINOR_VERTICAL);
        int fieldMinorTwo = lattice.addVolumeField(minorTwo, MINOR_HORIZONTAL, MINOR_VERTICAL);
        int fieldRooms = lattice.addVolumeField(rooms, ROOM_HORIZONTAL, ROOM_VERTICAL);
        int fieldEntrances = lattice.addColumnField(entrances, ENTRANCE_SIZE);

        BlockRegion densityRegion = density.getWorldRegion();
        lattice.prepare(face, densityRegion);

        ElevationFacet elevationFacet = region.getRegionFacet(ElevationFacet.class);
        // Clamped for the same reason DensityNoiseProvider clamps: the density region can come out
        // wider than the elevation area, borders being propagated in one reverse pass over a
        // provider list whose order is not a guaranteed topological order.
        BlockAreac elevationArea = elevationFacet.getWorldArea();
        int seaLevel = region.getRegionFacet(SeaLevelFacet.class).getSeaLevel();

        double mainThreshold = thresholdFor(MAIN_FRACTION);
        double minorThreshold = thresholdFor(MINOR_FRACTION);

        int x = densityRegion.minX();
        int y = densityRegion.minY();
        int z = densityRegion.minZ();
        for (int i = 0; i < densityValues.length; i++) {
            if (densityValues[i] > 0) {
                int ex = Math.max(elevationArea.minX(), Math.min(elevationArea.maxX(), x));
                int ez = Math.max(elevationArea.minY(), Math.min(elevationArea.maxY(), z));
                float elevation = elevationFacet.getWorld(ex, ez);
                double radius = radiusAt(lattice, fieldRooms, fieldEntrances, x, y, z, elevation, seaLevel);
                if (radius > 0.0 && isCarved(lattice, x, y, z, radius,
                        fieldMainOne, fieldMainTwo, mainThreshold,
                        fieldMinorOne, fieldMinorTwo, minorThreshold)) {
                    densityValues[i] = CARVED;
                    carved[i] = true;
                }
            }

            x++;
            if (x > densityRegion.maxX()) {
                x = densityRegion.minX();
                y++;
                if (y > densityRegion.maxY()) {
                    y = densityRegion.minY();
                    z++;
                }
            }
        }

        eraseCarvedSurfaces(region.getRegionFacet(SurfacesFacet.class), caves);
        region.setRegionFacet(CaveFacet.class, caves);
    }

    /**
     * Surfaces are taken away and never added, which is the whole of the flora problem.
     * <p>
     * Recomputing them the way {@code DensityNoiseProvider} does would mark every gallery floor a
     * surface, and {@code SurfaceObjectProvider} walks <em>all</em> the surfaces of a column with no
     * notion of open sky: there would be grass and trees underground. Removing instead also settles
     * the other half — a surface whose ground a tunnel has eaten leaves a tree standing in the air,
     * and {@code TreeRasterizer} does not check that the space is free before it builds.
     */
    private static void eraseCarvedSurfaces(SurfacesFacet surfaces, CaveFacet caves) {
        BlockRegion carvedRegion = caves.getWorldRegion();
        for (Vector3ic pos : surfaces.getWorldRegion()) {
            if (carvedRegion.contains(pos) && surfaces.getWorld(pos) && caves.getWorld(pos)) {
                surfaces.setWorld(pos, false);
            }
        }
    }

    private static boolean hasRock(float[] densityValues) {
        for (float value : densityValues) {
            if (value > 0) {
                return true;
            }
        }
        return false;
    }

    /** The threshold that carves a given fraction of the rock, from the Rayleigh distribution. */
    private static double thresholdFor(double fraction) {
        return SIGMA_ONE_OCTAVE * Math.sqrt(-2.0 * Math.log(1.0 - fraction));
    }

    /**
     * How wide the galleries run here, as a multiple of their nominal bore. Zero means no cave is
     * allowed at all.
     */
    private double radiusAt(SphereFieldLattice lattice, int fieldRooms, int fieldEntrances,
                            int x, int y, int z, float elevation, int seaLevel) {
        // A hard floor, and not merely a profile that fades to nothing: a profile is a floating
        // point multiplication, and the last rock above the mantle should not depend on a rounding.
        if (y < geometry.floorY() + FLOOR_MARGIN) {
            return 0.0;
        }
        double depth = elevation - y;
        if (depth <= 0.0) {
            return 0.0;
        }

        double cover = ROOF_COVER;
        double fade = ROOF_FADE;
        if (elevation < seaLevel + 2) {
            cover = SEA_FLOOR_SEAL;
        } else {
            double openness = clamp((lattice.column(fieldEntrances, x, z) - ENTRANCE_LEVEL) / ENTRANCE_SOFT);
            cover -= ROOF_COVER * openness;
            fade -= (ROOF_FADE - ROOF_COVER) * openness;
        }
        double roof = clamp((depth - cover) / fade);
        if (roof <= 0.0) {
            return 0.0;
        }

        double floor = clamp((y - geometry.floorY() - FLOOR_MARGIN) / FLOOR_FADE);
        double deep = 1.0 + DEEP_GAIN * clamp(-y / DEEP_AT);
        double swell = 1.0 + ROOM_SWELL
                * clamp((lattice.volume(fieldRooms, x, y, z) - ROOM_LEVEL) / (ROOM_PEAK - ROOM_LEVEL));
        return roof * floor * deep * swell;
    }

    private static boolean isCarved(SphereFieldLattice lattice, int x, int y, int z, double radius,
                                    int mainOneField, int mainTwoField, double mainThreshold,
                                    int minorOneField, int minorTwoField, double minorThreshold) {
        double main = mainThreshold * radius;
        double a = lattice.volume(mainOneField, x, y, z);
        double b = lattice.volume(mainTwoField, x, y, z);
        if (a * a + b * b < main * main) {
            return true;
        }
        double minor = minorThreshold * radius;
        double c = lattice.volume(minorOneField, x, y, z);
        double d = lattice.volume(minorTwoField, x, y, z);
        return c * c + d * d < minor * minor;
    }

    private static double clamp(double value) {
        return Math.max(0.0, Math.min(1.0, value));
    }
}
