// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.terasology.engine.network.Replicate;
import org.terasology.gestalt.entitysystem.component.Component;

/**
 * Who a hero is: the body chosen at the forge, the class taken from the rack, and the name.
 * <p>
 * <strong>Sealed once.</strong> A character arrives in a world unsealed, and the creator opens on it; the baptism
 * fixes everything at once and for good (see {@code LegendeAuthoritySystem}). Only an operator can start over.
 * <p>
 * The appearance is kept field by field rather than as a painted sheet: every client paints it again, so a sheet
 * never travels, and a hero two clients see is the same hero by construction.
 */
public class LegendeComponent implements Component<LegendeComponent> {

    @Replicate
    public boolean scelle;

    @Replicate
    public String nom = "";

    /** Prefab of the class taken at creation. */
    @Replicate
    public String classe = "";

    /** The split of D60, out of a hundred: the class's at creation, the player's own afterwards. */
    @Replicate
    public int melee;
    @Replicate
    public int ranged;
    @Replicate
    public int magic;

    @Replicate
    public String espece = "humain";
    @Replicate
    public int variante;
    @Replicate
    public float taille = 1.75f;
    @Replicate
    public float corpulence = 0.5f;
    @Replicate
    public String peau = "";
    @Replicate
    public String formeTete = "";
    @Replicate
    public String yeux = "";
    @Replicate
    public String couleurYeux = "";
    @Replicate
    public String bouche = "";
    @Replicate
    public String marques = "";
    @Replicate
    public String coiffure = "";
    @Replicate
    public String couleurCheveux = "";
    @Replicate
    public String barbe = "";
    @Replicate
    public String tenue = "";
    @Replicate
    public String couleurTenue = "";

    /** @return the appearance as the painter reads it */
    public Apparence apparence() {
        Apparence a = new Apparence();
        a.nom = nom;
        a.espece = espece;
        a.variante = variante;
        a.taille = taille;
        a.corpulence = corpulence;
        a.peau = peau;
        a.formeTete = formeTete;
        a.yeux = yeux;
        a.couleurYeux = couleurYeux;
        a.bouche = bouche;
        a.marques = marques;
        a.coiffure = coiffure;
        a.couleurCheveux = couleurCheveux;
        a.barbe = barbe;
        a.tenue = tenue;
        a.couleurTenue = couleurTenue;
        return a;
    }

    /** Takes every appearance field, the name included. */
    public void prendre(Apparence a) {
        nom = a.nom;
        espece = a.espece;
        variante = a.variante;
        taille = (float) a.taille;
        corpulence = (float) a.corpulence;
        peau = a.peau;
        formeTete = a.formeTete;
        yeux = a.yeux;
        couleurYeux = a.couleurYeux;
        bouche = a.bouche;
        marques = a.marques;
        coiffure = a.coiffure;
        couleurCheveux = a.couleurCheveux;
        barbe = a.barbe;
        tenue = a.tenue;
        couleurTenue = a.couleurTenue;
    }

    @Override
    public void copyFrom(LegendeComponent other) {
        this.scelle = other.scelle;
        this.classe = other.classe;
        this.melee = other.melee;
        this.ranged = other.ranged;
        this.magic = other.magic;
        prendre(other.apparence());
    }
}
