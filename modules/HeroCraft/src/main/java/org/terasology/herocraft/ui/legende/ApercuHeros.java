// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Quaternionf;
import org.joml.Vector2i;
import org.joml.Vector3f;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.assets.texture.Texture;
import org.terasology.engine.rendering.nui.CanvasUtility;
import org.terasology.engine.utilities.Assets;
import org.terasology.heroclasses.legende.Apparence;
import org.terasology.heroclasses.legende.Peaux;
import org.terasology.heroclasses.legende.Peintre;
import org.terasology.joml.geom.Rectanglei;
import org.terasology.nui.BaseInteractionListener;
import org.terasology.nui.Canvas;
import org.terasology.nui.CoreWidget;
import org.terasology.nui.InteractionListener;
import org.terasology.nui.events.NUIMouseClickEvent;
import org.terasology.nui.events.NUIMouseDragEvent;
import org.terasology.nui.events.NUIMouseReleaseEvent;

/**
 * The creator's turning figure: the sheet being shaped, on the hero's body, three-quarter on and slowly turning.
 * Dragging turns it by hand.
 * <p>
 * Heights compare: the figure is drawn at its species' size against a two-metre frame, feet on the same line, so a
 * gnome reads as small beside where a half-orc would stand. The build widens it as it widens the hero in the
 * world.
 */
public class ApercuHeros extends CoreWidget {

    /** The tallest hero the frame holds, in metres. */
    private static final float CADRE = 2.05f;
    /** Turns per second when left alone: a slow, legible turn, the mock-up's pace. */
    private static final float VITESSE = 0.55f;
    private static final float DEPART = (float) Math.toRadians(46);
    private static final float PENTE = (float) Math.toRadians(-4);
    /** Share of the shorter side the drawing fits the model's height into — the renderer's own fit. */
    private static final float AJUSTEMENT = 0.7f;

    private Mesh mesh;
    private Material materiau;
    private Texture texture;
    private String peinte = "";
    private float taille = 1.75f;
    private float largeur = -1;
    private float largeurVoulue = 1;

    private float angle = DEPART;
    private float pente = PENTE;
    private boolean tourne = true;
    private boolean saisi;
    private Vector2i dernier;

    private final InteractionListener saisie = new BaseInteractionListener() {
        @Override
        public boolean onMouseClick(NUIMouseClickEvent event) {
            saisi = true;
            dernier = new Vector2i(event.getRelativeMousePosition());
            return true;
        }

        @Override
        public void onMouseDrag(NUIMouseDragEvent event) {
            Vector2i ici = new Vector2i(event.getRelativeMousePosition());
            if (dernier != null) {
                angle += (ici.x - dernier.x) * 0.0096f;
                pente = Math.max(-0.42f, Math.min(0.42f, pente - (ici.y - dernier.y) * 0.007f));
            }
            dernier = ici;
        }

        @Override
        public void onMouseRelease(NUIMouseReleaseEvent event) {
            saisi = false;
            dernier = null;
        }
    };

    /** Shows this appearance; the sheet is painted again only if something painted on it has changed. */
    public void montrer(Apparence apparence) {
        taille = (float) apparence.taille;
        // the world's widening, see HerosVisuelSystem
        largeurVoulue = 0.92f + 0.16f * (float) apparence.corpulence;
        String cle = String.join("|", apparence.espece, Integer.toString(apparence.variante), apparence.peau,
                apparence.formeTete, apparence.yeux, apparence.couleurYeux, apparence.bouche, apparence.marques,
                apparence.coiffure, apparence.couleurCheveux, apparence.barbe, apparence.tenue,
                apparence.couleurTenue);
        if (texture == null || texture.isDisposed() || !cle.equals(peinte)) {
            texture = Peaux.texture("apercu", Peintre.peindre(apparence));
            peinte = cle;
        }
    }

    public boolean isTourne() {
        return tourne;
    }

    public void setTourne(boolean tourne) {
        this.tourne = tourne;
    }

    @Override
    public void update(float delta) {
        super.update(delta);
        if (tourne && !saisi) {
            angle += delta * VITESSE;
        }
    }

    @Override
    public void onDraw(Canvas canvas) {
        canvas.addInteractionRegion(saisie);
        if (texture == null) {
            return;
        }
        if (mesh == null || mesh.isDisposed() || largeur != largeurVoulue) {
            if (mesh != null && !mesh.isDisposed()) {
                mesh.dispose();
            }
            mesh = Figure.build(largeurVoulue);
            largeur = largeurVoulue;
        }
        if (materiau == null || materiau.isDisposed()) {
            materiau = Assets.getMaterial("HeroCraft:apercuHeros").orElse(null);
            if (materiau == null) {
                return;
            }
        }
        materiau.setTexture("tex", texture);

        Rectanglei region = canvas.getRegion();
        float echelle = taille / CADRE;
        // The renderer centres the model and fits its height; this puts the feet back on one line whatever the
        // height, by shifting down what the scale took off the bottom.
        float hauteur = AJUSTEMENT * Math.min(region.lengthX(), region.lengthY());
        Vector3f decalage = new Vector3f(0, hauteur * (1 - echelle) / 2, 0);
        Quaternionf rotation = new Quaternionf().rotateX(pente).rotateY(angle);
        CanvasUtility.drawMesh(canvas, mesh, materiau, region, rotation, decalage, echelle);
    }

    @Override
    public Vector2i getPreferredContentSize(Canvas canvas, Vector2i sizeHint) {
        return new Vector2i(sizeHint);
    }
}
