// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;
import java.util.Optional;
import java.util.function.Function;

/**
 * What the server accepts from the creator, checked against the atlas alone so that it can be tested without an
 * engine.
 * <p>
 * Every option must be one the creator offers: a species, a sub-race within it, a skin from its palette, a height
 * within its range, and the listed shapes and colours. What a sub-race imposes — a dwarf's beard, a hairstyle —
 * is imposed again here, as the creator does; what it merely suggests (eyes, mouth, head) stays the player's.
 */
public final class Bapteme {

    /** Longest name a hero may carry. */
    public static final int NOM_MAX = 32;

    private Bapteme() {
    }

    /**
     * @return the reason the appearance is refused, in French for the player, or empty when it is accepted — in
     *         which case {@code a} has been normalised in place (name trimmed, imposed traits applied)
     */
    public static Optional<String> verifier(Apparence a) {
        a.nom = a.nom == null ? "" : a.nom.trim();
        if (a.nom.isEmpty()) {
            return Optional.of("le héros n'a pas de nom");
        }
        if (a.nom.length() > NOM_MAX || a.nom.chars().anyMatch(Character::isISOControl)) {
            return Optional.of("ce nom ne peut pas se porter");
        }
        Espece espece = Atlas.ESPECES.get(a.espece);
        if (espece == null) {
            return Optional.of("espèce inconnue");
        }
        if (a.variante < 0 || a.variante >= espece.variantes().size()) {
            return Optional.of("variante inconnue");
        }
        if (!(a.taille >= espece.min() - 1e-6 && a.taille <= espece.max() + 1e-6)) {
            return Optional.of("taille hors de l'espèce");
        }
        if (!(a.corpulence >= 0 && a.corpulence <= 1)) {
            return Optional.of("corpulence hors d'échelle");
        }
        Traits traits = Atlas.traitsDe(a);
        if (present(traits.coiffureForcee())) {
            a.coiffure = traits.coiffureForcee();
        }
        if (present(traits.barbeForcee())) {
            a.barbe = traits.barbeForcee();
        }
        return premier(
                dans(a.peau, Atlas.peauxDe(a), "couleur de peau"),
                dans(a.formeTete, Atlas.FORMES_TETE, FormeTete::id, "forme de tête"),
                dans(a.yeux, Atlas.YEUX, Expression::id, "yeux"),
                dans(a.couleurYeux, Atlas.PAL_YEUX, "couleur des yeux"),
                dans(a.bouche, Atlas.BOUCHES, Expression::id, "bouche"),
                dans(a.marques, Atlas.MARQUES, Marque::id, "marques"),
                dans(a.coiffure, Atlas.COIFFURES, Coiffure::id, "coiffure"),
                dans(a.couleurCheveux, Atlas.PAL_CHEVEUX, "couleur des cheveux"),
                dans(a.barbe, Atlas.BARBES, Barbe::id, "barbe"),
                dans(a.tenue, Atlas.TENUES, Tenue::id, "tenue"),
                dans(a.couleurTenue, Atlas.PAL_TENUE, "couleur de tenue"));
    }

    @SafeVarargs
    private static Optional<String> premier(Optional<String>... raisons) {
        for (Optional<String> raison : raisons) {
            if (raison.isPresent()) {
                return raison;
            }
        }
        return Optional.empty();
    }

    private static Optional<String> dans(String valeur, List<String> permis, String quoi) {
        return permis.contains(valeur) ? Optional.empty() : Optional.of("choix inconnu : " + quoi);
    }

    private static <T> Optional<String> dans(String valeur, List<T> permis, Function<T, String> id, String quoi) {
        return permis.stream().map(id).anyMatch(v -> v.equals(valeur)) ? Optional.empty()
                : Optional.of("choix inconnu : " + quoi);
    }

    private static boolean present(String value) {
        return value != null && !value.isEmpty();
    }
}
