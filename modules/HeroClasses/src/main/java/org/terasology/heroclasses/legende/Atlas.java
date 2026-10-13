// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

/**
 * The hero atlas: the 64x64 two-layer pattern, every appearance option, the species and their sub-races, and the
 * rules that merge a species with its sub-race. Ported from the reference mock-up
 * {@code tools/heros/maquette/hero-atlas.js}; the data comes from {@link AtlasDonnees}, generated from it.
 * <p>
 * A sub-race lays its traits and proportions over its species' as {@code Object.assign} does: a field it names
 * replaces the species' one, a field it leaves out keeps it.
 */
public final class Atlas {

    /** Texture the skin is bound to. */
    public static final String IMAGE = AtlasDonnees.IMAGE;
    /** Blocks per texel of the pattern. */
    public static final double UNITE_EN_BLOCS = AtlasDonnees.UNITE_EN_BLOCS;

    /** The pattern's parts by id: six boxes per layer, then the wing, beard, hair, appendage and tail pieces. */
    public static final Map<String, Partie> PATRON = AtlasDonnees.patron();
    /** The six faces of a box, in the pattern's order. */
    public static final List<String> FACES = AtlasDonnees.faces();
    public static final List<Coiffure> COIFFURES = AtlasDonnees.coiffures();
    public static final List<Expression> YEUX = AtlasDonnees.yeux();
    public static final List<Expression> BOUCHES = AtlasDonnees.bouches();
    public static final List<Marque> MARQUES = AtlasDonnees.marques();
    public static final List<Barbe> BARBES = AtlasDonnees.barbes();
    public static final List<Tenue> TENUES = AtlasDonnees.tenues();
    public static final List<FormeTete> FORMES_TETE = AtlasDonnees.formesTete();
    public static final List<String> PAL_CHEVEUX = AtlasDonnees.palCheveux();
    public static final List<String> PAL_YEUX = AtlasDonnees.palYeux();
    public static final List<String> PAL_TENUE = AtlasDonnees.palTenue();
    /** Species by id. */
    public static final Map<String, Espece> ESPECES = indexer(AtlasDonnees.especes());
    /** Species ids in the order the editor lists them. */
    public static final List<String> ORDRE_ESPECES = AtlasDonnees.ordreEspeces();

    private Atlas() {
    }

    private static Map<String, Espece> indexer(List<Espece> especes) {
        Map<String, Espece> map = new LinkedHashMap<>();
        for (Espece espece : especes) {
            map.put(espece.id(), espece);
        }
        return Collections.unmodifiableMap(map);
    }

    /** @return the species of that id, or the human when the id is unknown */
    public static Espece espece(String id) {
        Espece espece = id == null ? null : ESPECES.get(id);
        return espece != null ? espece : ESPECES.get("humain");
    }

    /** @return the sub-race the appearance names, clamped to the last one; the first when out of range */
    public static Variante varianteDe(Apparence cfg) {
        List<Variante> variantes = espece(cfg.espece).variantes();
        if (variantes.isEmpty()) {
            return null;
        }
        int index = Math.min(cfg.variante, variantes.size() - 1);
        return index >= 0 ? variantes.get(index) : variantes.get(0);
    }

    /** @return the skin palette: the sub-race's own, else the species' */
    public static List<String> peauxDe(Apparence cfg) {
        Variante variante = varianteDe(cfg);
        return variante != null && variante.peaux() != null ? variante.peaux() : espece(cfg.espece).peaux();
    }

    /** @return the species' proportions with the sub-race's laid over them */
    public static Mods modsDe(Apparence cfg) {
        Variante variante = varianteDe(cfg);
        return espece(cfg.espece).mods().fusion(variante != null ? variante.mods() : null);
    }

    /** @return the species' traits with the sub-race's laid over them, relief and alpha defaulting to 1 */
    public static Traits traitsDe(Apparence cfg) {
        Variante variante = varianteDe(cfg);
        return new Traits().relief(1).alpha(1)
                .fusion(espece(cfg.espece).traits())
                .fusion(variante != null ? variante.traits() : null);
    }

    /** @return the default appearance of a species: its first sub-race, with whatever that sub-race imposes */
    public static Apparence defaut(String especeId) {
        Espece sp = espece(especeId);
        Variante premiere = sp.variantes().isEmpty() ? null : sp.variantes().get(0);
        Traits t = sp.traits().fusion(premiere != null ? premiere.traits() : null);
        List<String> pal = premiere != null && premiere.peaux() != null ? premiere.peaux() : sp.peaux();
        Apparence c = new Apparence();
        c.nom = "Sans nom";
        c.espece = sp.id();
        c.variante = 0;
        c.taille = sp.taille();
        c.corpulence = 0.5;
        c.peau = pal.size() > 1 ? pal.get(1) : pal.get(0);
        c.formeTete = ou(t.formeTeteForcee(), "carree");
        c.yeux = ou(t.yeuxForces(), "amande");
        c.couleurYeux = PAL_YEUX.get(0);
        c.bouche = ou(t.boucheForcee(), "neutre");
        c.marques = "aucune";
        c.coiffure = ou(t.coiffureForcee(), "courte");
        c.couleurCheveux = PAL_CHEVEUX.get(1);
        c.barbe = ou(t.barbeForcee(), "aucune");
        c.tenue = "tunique";
        c.couleurTenue = PAL_TENUE.get(0);
        return c;
    }

    /** @return a random appearance of a random species */
    public static Apparence aleatoire(Random random) {
        return aleatoire(random, null);
    }

    /**
     * A random appearance. Draws in the mock-up's order, one {@link Random#nextDouble()} per draw, and skips the
     * draws a sub-race's imposed traits make useless.
     *
     * @param especeId the species, or null for a random one
     */
    public static Apparence aleatoire(Random random, String especeId) {
        String id = present(especeId) ? especeId : pick(random, ORDRE_ESPECES);
        Espece sp = espece(id);
        Apparence c = defaut(id);
        c.variante = (int) Math.floor(random.nextDouble() * sp.variantes().size());
        Traits t = traitsDe(c);
        c.taille = Math.round((sp.min() + random.nextDouble() * (sp.max() - sp.min())) * 100) / 100.0;
        c.corpulence = Math.round(random.nextDouble() * 100) / 100.0;
        c.peau = pick(random, peauxDe(c));
        c.formeTete = present(t.formeTeteForcee()) ? t.formeTeteForcee() : pick(random, FORMES_TETE).id();
        c.yeux = present(t.yeuxForces()) ? t.yeuxForces() : pick(random, YEUX).id();
        c.couleurYeux = pick(random, PAL_YEUX);
        c.bouche = present(t.boucheForcee()) ? t.boucheForcee() : pick(random, BOUCHES).id();
        c.marques = pick(random, MARQUES).id();
        c.coiffure = present(t.coiffureForcee()) ? t.coiffureForcee() : pick(random, COIFFURES).id();
        c.couleurCheveux = pick(random, PAL_CHEVEUX);
        c.barbe = present(t.barbeForcee()) ? t.barbeForcee() : pick(random, BARBES).id();
        c.tenue = pick(random, TENUES).id();
        c.couleurTenue = pick(random, PAL_TENUE);
        return c;
    }

    private static <T> T pick(Random random, List<T> list) {
        return list.get((int) Math.floor(random.nextDouble() * list.size()));
    }

    private static String ou(String value, String fallback) {
        return present(value) ? value : fallback;
    }

    private static boolean present(String value) {
        return value != null && !value.isEmpty();
    }

    /** @return the body's height in texels: head, torso and legs, scaled by the proportions */
    public static double hauteurTexels(Apparence cfg) {
        Mods m = modsDe(cfg);
        return 8 * nonNul(m.tete()) + 12 + 12 * nonNul(m.jambes());
    }

    /**
     * The real height in texels: cranial appendages rise 8 texels above the skull. This, not the body, sets the
     * preview's scale, or ears and horns leave the frame.
     */
    public static double hauteurTotaleTexels(Apparence cfg) {
        String appendice = traitsDe(cfg).appendice();
        return hauteurTexels(cfg) + (present(appendice) ? 8 : 0);
    }

    /** @return blocks per texel */
    public static double metresParTexel() {
        return UNITE_EN_BLOCS;
    }

    private static double nonNul(Double value) {
        return value != null && value != 0 ? value : 1;
    }

    /** @return the hairstyle of that id, or null */
    public static Coiffure coiffure(String id) {
        return COIFFURES.stream().filter(o -> o.id().equals(id)).findFirst().orElse(null);
    }

    /** @return the eye shape of that id, or null */
    public static Expression yeux(String id) {
        return YEUX.stream().filter(o -> o.id().equals(id)).findFirst().orElse(null);
    }

    /** @return the mouth of that id, or null */
    public static Expression bouche(String id) {
        return BOUCHES.stream().filter(o -> o.id().equals(id)).findFirst().orElse(null);
    }

    /** @return the mark of that id, or null */
    public static Marque marque(String id) {
        return MARQUES.stream().filter(o -> o.id().equals(id)).findFirst().orElse(null);
    }

    /** @return the beard of that id, or null */
    public static Barbe barbe(String id) {
        return BARBES.stream().filter(o -> o.id().equals(id)).findFirst().orElse(null);
    }

    /** @return the outfit of that id, or null */
    public static Tenue tenue(String id) {
        return TENUES.stream().filter(o -> o.id().equals(id)).findFirst().orElse(null);
    }
}
