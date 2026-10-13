// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

/**
 * Traits of a species or sub-race, as the mock-up's loose {@code traits} map: every field is optional and null
 * when absent, so that a sub-race overrides only what it names (an explicit {@code false} does override). The
 * painter reads them with the mock-up's truthiness: null, zero and false all mean "no".
 */
public final class Traits {

    private Double relief;
    private Double alpha;
    private Double pelage;
    private Integer oreilles;
    private Integer canines;
    private Integer veines;
    private Integer barbeMin;
    private String motif;
    private String accent;
    private String reflet;
    private String lueur;
    private String regard;
    private String appendice;
    private String queue;
    private String oreillesAnimales;
    private String coiffureForcee;
    private String barbeForcee;
    private String yeuxForces;
    private String formeTeteForcee;
    private String boucheForcee;
    private Boolean mainsMassives;
    private Boolean machoire;
    private Boolean machoireLourde;
    private Boolean craquelee;
    private Boolean chevelureEtheree;
    private Boolean halo;
    private Boolean griffes;

    /** Shading depth of the face, or null; {@link Atlas#traitsDe} defaults it to 1. */
    public Double relief() {
        return relief;
    }

    /** Opacity of layer 1, or null; {@link Atlas#traitsDe} defaults it to 1. */
    public Double alpha() {
        return alpha;
    }

    /** Fur strength: grainier skin, tufts on the head, or null. */
    public Double pelage() {
        return pelage;
    }

    /** Length of pointed ears, in texels, or null. */
    public Integer oreilles() {
        return oreilles;
    }

    /** Number of lower canine rows showing (1 or 2), or null. */
    public Integer canines() {
        return canines;
    }

    /** Dark veins: 1 sparse, 2 dense, or null. */
    public Integer veines() {
        return veines;
    }

    /** Lowest beard index the species may pick (for the editor), or null. */
    public Integer barbeMin() {
        return barbeMin;
    }

    /** Body pattern painted over the skin, or null. */
    public String motif() {
        return motif;
    }

    /** Accent colour of the pattern and appendages, or null. */
    public String accent() {
        return accent;
    }

    /** Metallic sheen colour, or null. */
    public String reflet() {
        return reflet;
    }

    /** Colour of glowing eyes, or null. */
    public String lueur() {
        return lueur;
    }

    /** Eye kind overriding the shape: lumineux or vitreux, or null. */
    public String regard() {
        return regard;
    }

    /** Cranial appendage: ears, horns, antlers, crown, or null. */
    public String appendice() {
        return appendice;
    }

    /** Tail kind: longue, touffue or panache, or null. */
    public String queue() {
        return queue;
    }

    /** Animal ears painted on the head itself: lupin or another kind, or null. */
    public String oreillesAnimales() {
        return oreillesAnimales;
    }

    /** Hairstyle imposed by the sub-race, or null. */
    public String coiffureForcee() {
        return coiffureForcee;
    }

    /** Beard imposed by the sub-race, or null. */
    public String barbeForcee() {
        return barbeForcee;
    }

    /** Eye shape imposed by the sub-race, or null. */
    public String yeuxForces() {
        return yeuxForces;
    }

    /** Head shape imposed by the sub-race, or null. */
    public String formeTeteForcee() {
        return formeTeteForcee;
    }

    /** Mouth imposed by the sub-race, or null. */
    public String boucheForcee() {
        return boucheForcee;
    }

    /** Three rows of hand instead of two, or null. */
    public Boolean mainsMassives() {
        return mainsMassives;
    }

    /** Shadowed jaw line, or null. */
    public Boolean machoire() {
        return machoire;
    }

    /** Heavier jaw: two shadowed rows, or null. */
    public Boolean machoireLourde() {
        return machoireLourde;
    }

    /** Darker veins on cracked skin, or null. */
    public Boolean craquelee() {
        return craquelee;
    }

    /** Lighter, ethereal hair, or null. */
    public Boolean chevelureEtheree() {
        return chevelureEtheree;
    }

    /** Halo painted on the back, or null. */
    public Boolean halo() {
        return halo;
    }

    /** Claws at the tips of the hands, or null. */
    public Boolean griffes() {
        return griffes;
    }

    Traits relief(double value) {
        this.relief = value;
        return this;
    }

    Traits alpha(double value) {
        this.alpha = value;
        return this;
    }

    Traits pelage(double value) {
        this.pelage = value;
        return this;
    }

    Traits oreilles(int value) {
        this.oreilles = value;
        return this;
    }

    Traits canines(int value) {
        this.canines = value;
        return this;
    }

    Traits veines(int value) {
        this.veines = value;
        return this;
    }

    Traits barbeMin(int value) {
        this.barbeMin = value;
        return this;
    }

    Traits motif(String value) {
        this.motif = value;
        return this;
    }

    Traits accent(String value) {
        this.accent = value;
        return this;
    }

    Traits reflet(String value) {
        this.reflet = value;
        return this;
    }

    Traits lueur(String value) {
        this.lueur = value;
        return this;
    }

    Traits regard(String value) {
        this.regard = value;
        return this;
    }

    Traits appendice(String value) {
        this.appendice = value;
        return this;
    }

    Traits queue(String value) {
        this.queue = value;
        return this;
    }

    Traits oreillesAnimales(String value) {
        this.oreillesAnimales = value;
        return this;
    }

    Traits coiffureForcee(String value) {
        this.coiffureForcee = value;
        return this;
    }

    Traits barbeForcee(String value) {
        this.barbeForcee = value;
        return this;
    }

    Traits yeuxForces(String value) {
        this.yeuxForces = value;
        return this;
    }

    Traits formeTeteForcee(String value) {
        this.formeTeteForcee = value;
        return this;
    }

    Traits boucheForcee(String value) {
        this.boucheForcee = value;
        return this;
    }

    Traits mainsMassives(boolean value) {
        this.mainsMassives = value;
        return this;
    }

    Traits machoire(boolean value) {
        this.machoire = value;
        return this;
    }

    Traits machoireLourde(boolean value) {
        this.machoireLourde = value;
        return this;
    }

    Traits craquelee(boolean value) {
        this.craquelee = value;
        return this;
    }

    Traits chevelureEtheree(boolean value) {
        this.chevelureEtheree = value;
        return this;
    }

    Traits halo(boolean value) {
        this.halo = value;
        return this;
    }

    Traits griffes(boolean value) {
        this.griffes = value;
        return this;
    }

    /** @return a new set where every field {@code over} carries replaces this one's, as {@code Object.assign} does */
    public Traits fusion(Traits over) {
        Traits merged = copie();
        if (over == null) {
            return merged;
        }
        if (over.relief != null) {
            merged.relief = over.relief;
        }
        if (over.alpha != null) {
            merged.alpha = over.alpha;
        }
        if (over.pelage != null) {
            merged.pelage = over.pelage;
        }
        if (over.oreilles != null) {
            merged.oreilles = over.oreilles;
        }
        if (over.canines != null) {
            merged.canines = over.canines;
        }
        if (over.veines != null) {
            merged.veines = over.veines;
        }
        if (over.barbeMin != null) {
            merged.barbeMin = over.barbeMin;
        }
        if (over.motif != null) {
            merged.motif = over.motif;
        }
        if (over.accent != null) {
            merged.accent = over.accent;
        }
        if (over.reflet != null) {
            merged.reflet = over.reflet;
        }
        if (over.lueur != null) {
            merged.lueur = over.lueur;
        }
        if (over.regard != null) {
            merged.regard = over.regard;
        }
        if (over.appendice != null) {
            merged.appendice = over.appendice;
        }
        if (over.queue != null) {
            merged.queue = over.queue;
        }
        if (over.oreillesAnimales != null) {
            merged.oreillesAnimales = over.oreillesAnimales;
        }
        if (over.coiffureForcee != null) {
            merged.coiffureForcee = over.coiffureForcee;
        }
        if (over.barbeForcee != null) {
            merged.barbeForcee = over.barbeForcee;
        }
        if (over.yeuxForces != null) {
            merged.yeuxForces = over.yeuxForces;
        }
        if (over.formeTeteForcee != null) {
            merged.formeTeteForcee = over.formeTeteForcee;
        }
        if (over.boucheForcee != null) {
            merged.boucheForcee = over.boucheForcee;
        }
        if (over.mainsMassives != null) {
            merged.mainsMassives = over.mainsMassives;
        }
        if (over.machoire != null) {
            merged.machoire = over.machoire;
        }
        if (over.machoireLourde != null) {
            merged.machoireLourde = over.machoireLourde;
        }
        if (over.craquelee != null) {
            merged.craquelee = over.craquelee;
        }
        if (over.chevelureEtheree != null) {
            merged.chevelureEtheree = over.chevelureEtheree;
        }
        if (over.halo != null) {
            merged.halo = over.halo;
        }
        if (over.griffes != null) {
            merged.griffes = over.griffes;
        }
        return merged;
    }

    /** @return a copy of this set */
    public Traits copie() {
        Traits copy = new Traits();
        copy.relief = relief;
        copy.alpha = alpha;
        copy.pelage = pelage;
        copy.oreilles = oreilles;
        copy.canines = canines;
        copy.veines = veines;
        copy.barbeMin = barbeMin;
        copy.motif = motif;
        copy.accent = accent;
        copy.reflet = reflet;
        copy.lueur = lueur;
        copy.regard = regard;
        copy.appendice = appendice;
        copy.queue = queue;
        copy.oreillesAnimales = oreillesAnimales;
        copy.coiffureForcee = coiffureForcee;
        copy.barbeForcee = barbeForcee;
        copy.yeuxForces = yeuxForces;
        copy.formeTeteForcee = formeTeteForcee;
        copy.boucheForcee = boucheForcee;
        copy.mainsMassives = mainsMassives;
        copy.machoire = machoire;
        copy.machoireLourde = machoireLourde;
        copy.craquelee = craquelee;
        copy.chevelureEtheree = chevelureEtheree;
        copy.halo = halo;
        copy.griffes = griffes;
        return copy;
    }
}
