// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

/** Body proportions of a species or sub-race; every field is optional and null when absent. */
public final class Mods {

    private Double torse;
    private Double jambes;
    private Double bras;
    private Double tete;
    private Integer voute;
    private Boolean flotte;
    private Boolean ailes;
    private Boolean pointes;

    /** Torso scale, or null. */
    public Double torse() {
        return torse;
    }

    /** Leg scale, or null. */
    public Double jambes() {
        return jambes;
    }

    /** Arm scale, or null. */
    public Double bras() {
        return bras;
    }

    /** Head scale, or null. */
    public Double tete() {
        return tete;
    }

    /** Stoop, in degrees, or null. */
    public Integer voute() {
        return voute;
    }

    /** Floats above the ground, or null. */
    public Boolean flotte() {
        return flotte;
    }

    /** Spectral wings, or null. */
    public Boolean ailes() {
        return ailes;
    }

    /** Walks on tiptoe, or null. */
    public Boolean pointes() {
        return pointes;
    }

    Mods torse(double value) {
        this.torse = value;
        return this;
    }

    Mods jambes(double value) {
        this.jambes = value;
        return this;
    }

    Mods bras(double value) {
        this.bras = value;
        return this;
    }

    Mods tete(double value) {
        this.tete = value;
        return this;
    }

    Mods voute(int value) {
        this.voute = value;
        return this;
    }

    Mods flotte(boolean value) {
        this.flotte = value;
        return this;
    }

    Mods ailes(boolean value) {
        this.ailes = value;
        return this;
    }

    Mods pointes(boolean value) {
        this.pointes = value;
        return this;
    }

    /** @return a new set where every field {@code over} carries replaces this one's, as {@code Object.assign} does */
    public Mods fusion(Mods over) {
        Mods merged = copie();
        if (over == null) {
            return merged;
        }
        if (over.torse != null) {
            merged.torse = over.torse;
        }
        if (over.jambes != null) {
            merged.jambes = over.jambes;
        }
        if (over.bras != null) {
            merged.bras = over.bras;
        }
        if (over.tete != null) {
            merged.tete = over.tete;
        }
        if (over.voute != null) {
            merged.voute = over.voute;
        }
        if (over.flotte != null) {
            merged.flotte = over.flotte;
        }
        if (over.ailes != null) {
            merged.ailes = over.ailes;
        }
        if (over.pointes != null) {
            merged.pointes = over.pointes;
        }
        return merged;
    }

    /** @return a copy of this set */
    public Mods copie() {
        Mods copy = new Mods();
        copy.torse = torse;
        copy.jambes = jambes;
        copy.bras = bras;
        copy.tete = tete;
        copy.voute = voute;
        copy.flotte = flotte;
        copy.ailes = ailes;
        copy.pointes = pointes;
        return copy;
    }
}
