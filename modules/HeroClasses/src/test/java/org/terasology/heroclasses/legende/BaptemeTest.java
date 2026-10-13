// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.junit.jupiter.api.Test;

import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** What the server accepts from the creator, and what the forge keeps within bounds. */
public class BaptemeTest {

    @Test
    void chaqueEspeceParDefautEstAcceptee() {
        for (String id : Atlas.ORDRE_ESPECES) {
            Apparence a = nomme(Atlas.defaut(id));
            assertEquals("", Bapteme.verifier(a).orElse(""), id);
        }
    }

    @Test
    void lesTiragesSontAcceptes() {
        Random random = new Random(7);
        for (int i = 0; i < 200; i++) {
            Apparence a = nomme(Atlas.aleatoire(random));
            assertEquals("", Bapteme.verifier(a).orElse(""), "tirage " + i);
        }
    }

    @Test
    void leNomEstExigeEtRogne() {
        Apparence a = Atlas.defaut("humain");
        a.nom = "   ";
        assertTrue(Bapteme.verifier(a).isPresent());
        a.nom = "  Brandin  ";
        assertTrue(Bapteme.verifier(a).isEmpty());
        assertEquals("Brandin", a.nom);
        a.nom = "x".repeat(Bapteme.NOM_MAX + 1);
        assertTrue(Bapteme.verifier(a).isPresent());
        a.nom = "Bran\ndin";
        assertTrue(Bapteme.verifier(a).isPresent());
    }

    @Test
    void ceQueLeCreateurNOffrePasEstRefuse() {
        Apparence taille = nomme(Atlas.defaut("gnome"));
        taille.taille = 2.0;
        assertTrue(Bapteme.verifier(taille).isPresent());

        Apparence espece = nomme(Atlas.defaut("humain"));
        espece.espece = "dragon";
        assertTrue(Bapteme.verifier(espece).isPresent());

        Apparence variante = nomme(Atlas.defaut("humain"));
        variante.variante = 99;
        assertTrue(Bapteme.verifier(variante).isPresent());

        Apparence coiffure = nomme(Atlas.defaut("humain"));
        coiffure.coiffure = "crete-de-feu";
        assertTrue(Bapteme.verifier(coiffure).isPresent());

        Apparence couleur = nomme(Atlas.defaut("humain"));
        couleur.couleurTenue = "#123456";
        assertTrue(Bapteme.verifier(couleur).isPresent());
    }

    @Test
    void laForgeRameneLaTailleDansLEspece() {
        Apparence a = Atlas.defaut("humain");
        a.espece = "gnome";
        Forge.retoucher(a, true);
        Espece gnome = Atlas.espece("gnome");
        assertTrue(a.taille <= gnome.max() && a.taille >= gnome.min());
        assertTrue(Atlas.peauxDe(a).contains(a.peau));
        assertTrue(Bapteme.verifier(nomme(a)).isEmpty());
    }

    @Test
    void unNomTireSePorte() {
        Random random = new Random(3);
        for (int i = 0; i < 100; i++) {
            Apparence a = Atlas.defaut("humain");
            a.nom = Forge.nomAleatoire(random);
            assertTrue(Bapteme.verifier(a).isEmpty(), a.nom);
        }
    }

    private static Apparence nomme(Apparence a) {
        a.nom = "Torwen";
        return a;
    }
}
