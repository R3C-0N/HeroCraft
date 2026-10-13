// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.terasology.engine.network.ServerEvent;
import org.terasology.gestalt.entitysystem.event.Event;

/**
 * The creator's last button: the client sends every choice at once, and the server checks each one against the
 * atlas and the class before anything is fixed. Nothing here is trusted.
 */
@ServerEvent
public class BaptemeRequest implements Event {
    private String nom;
    private String classe;
    /** The arsenal item picked, or empty when the class offers no choice. */
    private String arme;
    private String espece;
    private int variante;
    private float taille;
    private float corpulence;
    private String peau;
    private String formeTete;
    private String yeux;
    private String couleurYeux;
    private String bouche;
    private String marques;
    private String coiffure;
    private String couleurCheveux;
    private String barbe;
    private String tenue;
    private String couleurTenue;

    public BaptemeRequest() {
    }

    public BaptemeRequest(Apparence a, String classe, String arme) {
        this.nom = a.nom;
        this.classe = classe;
        this.arme = arme == null ? "" : arme;
        this.espece = a.espece;
        this.variante = a.variante;
        this.taille = (float) a.taille;
        this.corpulence = (float) a.corpulence;
        this.peau = a.peau;
        this.formeTete = a.formeTete;
        this.yeux = a.yeux;
        this.couleurYeux = a.couleurYeux;
        this.bouche = a.bouche;
        this.marques = a.marques;
        this.coiffure = a.coiffure;
        this.couleurCheveux = a.couleurCheveux;
        this.barbe = a.barbe;
        this.tenue = a.tenue;
        this.couleurTenue = a.couleurTenue;
    }

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

    public String getClasse() {
        return classe;
    }

    public String getArme() {
        return arme;
    }
}
