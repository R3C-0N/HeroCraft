// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator;

import org.terasology.core.world.CoreBiome;

/**
 * Who built a ruin, read from where it stands.
 * <p>
 * D24 gives every zone of the map a people and a culture of its own. Until Q8 names them, a culture
 * is what its builders had to hand: the stone of the zone, laid in the zone's way. Four blocks say
 * it — the wall, the floor, the pillar and the anchor stone in the middle, where lot 2 opens the
 * window onto the past.
 * <p>
 * The sea, the abyss and the beaches have no culture: nobody built there, or nothing of it is left.
 * The block names stay strings so that the provider can decide without a block manager, which is
 * what lets a census measure the ruins with no engine running.
 */
public enum RuinCulture {

    TEMPERATE("CoreAssets:CobbleStone", "CoreAssets:Stone", "CoreAssets:HardStoneCut", "CoreAssets:Crystal",
            CoreBiome.PLAINS, CoreBiome.FOREST, CoreBiome.SWAMP),
    ARID("CoreAssets:Brick", "CoreAssets:FlintBrick", "CoreAssets:BrickWhitewashed", "CoreAssets:Crystal",
            CoreBiome.DESERT, CoreBiome.SAVANNA),
    BOREAL("CoreAssets:PackIce", "CoreAssets:HardStoneCut", "CoreAssets:Ice", "CoreAssets:Crystal",
            CoreBiome.SNOW, CoreBiome.TAIGA, CoreBiome.PACK_ICE, CoreBiome.ICE_SHELF),
    ALPINE("CoreAssets:FlintBrick", "CoreAssets:HardStoneCut", "CoreAssets:HardStone", "CoreAssets:Crystal",
            CoreBiome.MOUNTAINS),
    VOLCANIC("CoreAssets:Basalt", "CoreAssets:Obsidian", "CoreAssets:Basalt", "CoreAssets:Crystal",
            CoreBiome.VOLCANIC),
    ARCANE("CoreAssets:HardStoneCut", "CoreAssets:Crystal", "CoreAssets:Crystal", "CoreAssets:Obsidian",
            CoreBiome.ARCANE),
    MIASMA("CoreAssets:FlintBrick", "CoreAssets:Mud", "CoreAssets:Basalt", "CoreAssets:Crystal",
            CoreBiome.MIASMA);

    private final String wall;
    private final String floor;
    private final String pillar;
    private final String anchor;
    private final CoreBiome[] biomes;

    RuinCulture(String wall, String floor, String pillar, String anchor, CoreBiome... biomes) {
        this.wall = wall;
        this.floor = floor;
        this.pillar = pillar;
        this.anchor = anchor;
        this.biomes = biomes;
    }

    /** @return the culture of a biome, or null where nobody built */
    public static RuinCulture of(Object biome) {
        for (RuinCulture culture : values()) {
            for (CoreBiome candidate : culture.biomes) {
                if (candidate == biome) {
                    return culture;
                }
            }
        }
        return null;
    }

    /** The block a part of the ruin is made of. */
    public String blockOf(RuinShape.Part part) {
        switch (part) {
            case WALL:
            case FOUNDATION:
            case RUBBLE:
                return wall;
            case FLOOR:
                return floor;
            case PILLAR:
            case ALTAR:
                return pillar;
            case ANCHOR:
                return anchor;
            default:
                throw new IllegalArgumentException("no block for " + part);
        }
    }
}
