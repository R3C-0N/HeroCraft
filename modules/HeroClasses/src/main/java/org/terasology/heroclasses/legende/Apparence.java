// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

/**
 * What a hero looks like: the configuration the painter turns into a skin, field for field the mock-up's
 * {@code cfg} object. Option fields hold ids from {@link Atlas}; colours are {@code #rrggbb} strings.
 * <p>
 * Mutable on purpose, like the object it mirrors: the editor sets one field at a time.
 */
public final class Apparence {

    public String nom;
    public String espece;
    public int variante;
    /** Height in metres. */
    public double taille;
    /** Build, from 0 (slender) to 1 (stout). */
    public double corpulence;
    public String peau;
    public String formeTete;
    public String yeux;
    public String couleurYeux;
    public String bouche;
    public String marques;
    public String coiffure;
    public String couleurCheveux;
    public String barbe;
    public String tenue;
    public String couleurTenue;

    /** @return a copy of this appearance */
    public Apparence copie() {
        Apparence copy = new Apparence();
        copy.nom = nom;
        copy.espece = espece;
        copy.variante = variante;
        copy.taille = taille;
        copy.corpulence = corpulence;
        copy.peau = peau;
        copy.formeTete = formeTete;
        copy.yeux = yeux;
        copy.couleurYeux = couleurYeux;
        copy.bouche = bouche;
        copy.marques = marques;
        copy.coiffure = coiffure;
        copy.couleurCheveux = couleurCheveux;
        copy.barbe = barbe;
        copy.tenue = tenue;
        copy.couleurTenue = couleurTenue;
        return copy;
    }
}
