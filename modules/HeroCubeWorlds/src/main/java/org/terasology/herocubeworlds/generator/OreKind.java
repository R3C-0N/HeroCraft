// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator;

import org.terasology.context.annotation.API;

/**
 * The ten ores, and everything that decides where one is.
 * <p>
 * The whole design lives in this table rather than being spread over a provider, a rasterizer and a
 * census, because every one of these numbers is answerable to the others. The size of a vein is not
 * a free setting: it is {@code share * horizontal^2 * vertical}, and the count of veins in a chunk
 * column is {@code 1024 * height / (horizontal^2 * vertical)} — which depends on the field size and
 * <em>not</em> on the share. Shrink a vein and the ore is not rarer, it is diced.
 * <p>
 * <b>The thresholds here are measured, and a computed one would have been wrong by half.</b> Every
 * field is three octaves of the engine's simplex, whose raw width is sigma = 0.2736. A Gaussian
 * tail then puts three parts in a thousand at 2.75 sigma, which is 0.75. The generator does not
 * read the raw noise: it reads {@link SphereFieldLattice}, evaluated on a four-block lattice and
 * interpolated between the nodes, and interpolation averages peaks away. Measured on the field as
 * the provider actually sees it, sigma is 0.19 to 0.21 and the field never exceeds <b>0.81</b> —
 * so a threshold of 0.75 would have taken a few parts per million instead of three per thousand.
 * Copper would have been all but absent, and nothing on screen would have said so. The numbers in
 * the threshold column are quantiles printed by {@code OreFieldCalibrationTest}.
 * <p>
 * <b>Rarity is bought with a district, never by raising a threshold.</b> Not because the field runs
 * out — at these sizes it has room down to a part in ten thousand — but because a rarer threshold
 * only thins a vein, while a district decides <em>where</em> the ore is. It is the lesson of
 * {@code MOUTH_FLARE} again: the count is not the lever, the placement is.
 * <p>
 * <b>Tin was going to be copper's crown</b>, the shell between two thresholds of copper's own field:
 * no second noise, and the block itself asked for it — "found in the same veins as copper". The
 * census killed it twice over. A shell around a seven-block lump is a block and a half thick and
 * six-neighbour connectivity breaks it into crumbs, 2171 veins for 3086 blocks where the design
 * wanted four to a vein; and thickening it means widening the gap between the thresholds, which
 * multiplies the tonnage. A crown cannot be both thin and chunky, and that is arithmetic, not
 * tuning. Confining tin to copper's high ground instead worked no better: tin then sits inside every
 * copper lump by construction, so it ate four percent of the copper and came out at seven blocks to
 * a vein. Tin has a field of its own and keeps company with copper by depth alone — its band is the
 * lower half of copper's.
 */
@API
public enum OreKind {

    //     block                          horiz  vert  share   thresh  bottom  top              margin  edge          district           size
    COAL("CoreAssets:CoalOre",             18.0,  6.0, 0.0060, 0.4705,  -70, OreKind.SURFACE,      4, OreKind.FULL, District.NONE,       0.0),
    COPPER("CoreAssets:CopperOre",         15.0, 10.0, 0.0030, 0.4990,  -96, OreKind.SURFACE,      4, OreKind.FULL, District.NONE,       0.0),
    TIN("CoreAssets:TinOre",               15.0, 10.0, 0.0019, 0.5237,  -96, -30,                  0, OreKind.FULL, District.NONE,       0.0),
    IRON("CoreAssets:IronOre",             16.0, 11.0, 0.0025, 0.5132, -136, -8,                   0, OreKind.FULL, District.NONE,       0.0),
    GOLD("CoreAssets:GoldOre",             15.0, 10.0, 0.0030, 0.5048, -150, -70,                  0, 2,            District.OWN_FIELD, 380.0),
    AMBER("CoreAssets:AmberOre",           16.0,  6.0, 0.0030, 0.5082,  -16, OreKind.SURFACE,      6, 2,            District.WOODLAND,    0.0),
    LAPIS("CoreAssets:LapisOre",           15.0, 12.0, 0.0030, 0.5071, -144, -56,                  0, 2,            District.ARCANE,      0.0),
    TOPAZ("CoreAssets:TopazOre",           12.0,  8.0, 0.0030, 0.4884, -160, -72,                  0, 2,            District.VOLCANIC,    0.0),
    AQUAMARINE("CoreAssets:AquamarineOre", 13.0,  9.0, 0.0030, 0.4949, -120, -84,                  0, 1,            District.ABYSS,       0.0),
    DIAMOND("CoreAssets:DiamondOre",       12.0,  9.0, 0.0030, 0.4833, -180, -150,                 0, 1,            District.OWN_FIELD, 300.0);

    /**
     * A band whose top follows the ground rather than sitting at a fixed height.
     * <p>
     * Named above with its class in front, and it has to be: a constant declared after the enum
     * values cannot be reached by its simple name from one of their arguments.
     */
    public static final int SURFACE = Integer.MAX_VALUE;

    /** An edge height meaning the band never closes, because the ore has no district. */
    public static final int FULL = -1;

    private final String blockUri;
    private final double horizontalSize;
    private final double verticalSize;
    private final double share;
    private final double threshold;
    private final int coreBottom;
    private final int coreTop;
    private final int surfaceMargin;
    private final int edgeHeight;
    private final District district;
    private final double districtSize;

    OreKind(String blockUri, double horizontalSize, double verticalSize, double share, double threshold,
            int coreBottom, int coreTop, int surfaceMargin, int edgeHeight, District district, double districtSize) {
        this.blockUri = blockUri;
        this.horizontalSize = horizontalSize;
        this.verticalSize = verticalSize;
        this.share = share;
        this.threshold = threshold;
        this.coreBottom = coreBottom;
        this.coreTop = coreTop;
        this.surfaceMargin = surfaceMargin;
        this.edgeHeight = edgeHeight;
        this.district = district;
        this.districtSize = districtSize;
    }

    public String blockUri() {
        return blockUri;
    }

    /** The size of the noise features across the sphere, in blocks. */
    public double horizontalSize() {
        return horizontalSize;
    }

    /**
     * The size of the noise features with height, in blocks.
     * <p>
     * Squashed well below the horizontal one it gives a lens, which is the shape of a seam; near it,
     * a lump. See {@code SphereSampler.readAt} — the vertical size is what lays a field over.
     */
    public double verticalSize() {
        return verticalSize;
    }

    /**
     * The fraction of the opened band this ore should occupy.
     * <p>
     * Not read by the generator, which works off {@link #threshold()}. It is the target the census
     * measures against and the input the calibration test turns into a threshold — kept beside the
     * threshold so that the two cannot drift apart unnoticed.
     */
    public double share() {
        return share;
    }

    /** The field value above which a cell is this ore. */
    public double threshold() {
        return threshold;
    }

    public District district() {
        return district;
    }

    /** The feature size of an {@link District#OWN_FIELD} district, in blocks. */
    public double districtSize() {
        return districtSize;
    }

    /**
     * The lowest block of the band at the heart of the district.
     * <p>
     * Depth is the first lock and the tool grade only the second: gold asks for nothing better than
     * a flint pick and sits at -110. Nothing stops a player taking it; everything stops them getting
     * there.
     */
    public int coreBottom() {
        return coreBottom;
    }

    /**
     * The highest block of the band, given the ground above.
     * <p>
     * A band that followed the ground blindly would put coal in the grass. The margin keeps every
     * ore that far under the surface; what pushes it deeper still is the soil, which is not this
     * class's business — see {@code CoreBiome.rockDepth}.
     */
    public int coreTopAt(float surface) {
        return coreTop == SURFACE ? Math.round(surface) - surfaceMargin : coreTop;
    }

    /**
     * How tall the band is where the district is absent, in blocks.
     * <p>
     * {@link #FULL} for an ore with no district. A band always shrinks towards its own middle, so
     * that one rule covers a band anchored to a depth and a band anchored to the ground.
     */
    public int edgeHeight() {
        return edgeHeight;
    }

    /**
     * What opens an ore's band.
     * <p>
     * <b>A district forbids nothing; it opens a band.</b> At the heart of one the slice is its full
     * height, away from it the slice closes to a block or two — never to nothing. A hard condition
     * would make lapis exist only under the arcane, and mithril is alloyed with lapis: the stasis
     * sphere would become compulsory for a fantastic metal, and D15 says the seven base tiers do
     * not order one another. A band that closes keeps the region the right place to look without
     * making it the only one.
     * <p>
     * Four of the five read a field the world already carries, so they cost no noise of their own.
     */
    public enum District {
        /** No district: the band is its full height everywhere. */
        NONE,
        /** A column field of the ore's own, at its own district size. */
        OWN_FIELD,
        /** The arcane mask of {@code CubeRegionProvider}, against the threshold the biome uses. */
        ARCANE,
        /** The volcanic mask, likewise. Read as a mask and not as a biome: the biome wants land. */
        VOLCANIC,
        /** The abyssal sea floor, which is a biome rather than a mask. */
        ABYSS,
        /** Forest, taiga, swamp and miasma — where a resin could have fossilised. */
        WOODLAND
    }
}
