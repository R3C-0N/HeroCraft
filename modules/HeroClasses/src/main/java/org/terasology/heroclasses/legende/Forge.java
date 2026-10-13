// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;
import java.util.Random;

/**
 * The creator's rules for a body being shaped, apart from any screen: what a species or a variant imposes after a
 * change, and the names the forge offers. A port of {@code maj} and {@code nomAleatoire} from the mock-up.
 */
public final class Forge {

    private static final List<String> SYL1 = List.of("Bran", "Kae", "Tor", "El", "Var", "Mor", "Sig", "Dru", "Yl",
            "Gor", "Ash", "Nym", "Rhu", "Cal");
    private static final List<String> SYL2 = List.of("din", "wen", "gar", "mir", "ska", "thal", "ven", "run", "dra",
            "lin", "vok", "nara");
    private static final List<String> TITRES = List.of("de Valbrume", "du Chêne fendu", "des Marches basses",
            "aux Deux Clous", "de Bas-Fort", "la Dernière", "le Rouge", "sans Terre", "de la Forge froide",
            "aux Corbeaux");

    private Forge() {
    }

    /**
     * Brings an appearance back within what its species and variant allow, in place.
     *
     * @param souche whether the species or the variant has just changed: only then do the forced eyes, head and
     *               mouth replace the player's own pick, which they may otherwise change afterwards
     */
    public static Apparence retoucher(Apparence cfg, boolean souche) {
        Traits traits = Atlas.traitsDe(cfg);
        if (traits.coiffureForcee() != null) {
            cfg.coiffure = traits.coiffureForcee();
        }
        if (traits.barbeForcee() != null) {
            cfg.barbe = traits.barbeForcee();
        }
        if (souche) {
            if (traits.yeuxForces() != null) {
                cfg.yeux = traits.yeuxForces();
            }
            if (traits.formeTeteForcee() != null) {
                cfg.formeTete = traits.formeTeteForcee();
            }
            if (traits.boucheForcee() != null) {
                cfg.bouche = traits.boucheForcee();
            }
        }
        List<String> peaux = Atlas.peauxDe(cfg);
        if (!peaux.contains(cfg.peau)) {
            cfg.peau = peaux.size() > 1 ? peaux.get(1) : peaux.get(0);
        }
        Espece espece = Atlas.espece(cfg.espece);
        cfg.taille = Math.max(espece.min(), Math.min(espece.max(), cfg.taille));
        return cfg;
    }

    /** @return a name the way the garrison clerks write them, with a byname a little over half the time */
    public static String nomAleatoire(Random random) {
        String nom = pick(random, SYL1) + pick(random, SYL2);
        return random.nextDouble() < 0.55 ? nom + " " + pick(random, TITRES) : nom;
    }

    private static String pick(Random random, List<String> liste) {
        return liste.get(random.nextInt(liste.size()));
    }
}
