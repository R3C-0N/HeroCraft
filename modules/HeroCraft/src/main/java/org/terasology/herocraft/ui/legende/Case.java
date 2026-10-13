// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Vector2i;
import org.terasology.engine.rendering.assets.font.Font;
import org.terasology.engine.utilities.Assets;
import org.terasology.joml.geom.Rectanglei;
import org.terasology.nui.BaseInteractionListener;
import org.terasology.nui.Border;
import org.terasology.nui.Canvas;
import org.terasology.nui.Color;
import org.terasology.nui.CoreWidget;
import org.terasology.nui.HorizontalAlign;
import org.terasology.nui.InteractionListener;
import org.terasology.nui.UITextureRegion;
import org.terasology.nui.VerticalAlign;
import org.terasology.nui.events.NUIMouseClickEvent;

import java.util.function.BooleanSupplier;

/**
 * The mock-up's ticked row: a square that fills when chosen, a name, and on the right what sets it apart — the
 * height of a species, nothing for a way. Chosen, the row turns to raised wood; the others stay dark.
 * <p>
 * It draws itself rather than going through the skin: one widget, three states, and textures the skin already
 * names elsewhere.
 */
public class Case extends CoreWidget {

    private static final int HAUTEUR = 40;
    private static final int CARRE = 20;
    private static final Color TEXTE = new Color(0xFFF3DCFF);
    private static final Color VALEUR = new Color(0xB49E7CFF);
    private static final Color VALEUR_CHOISIE = new Color(0x3A2210FF);
    private static final Color REMPLI = new Color(0xF5B04AFF);
    private static final Color CREUX = new Color(0x231409FF);
    private static final Color CERCLE = new Color(0x9AA5ABFF);

    private String nom = "";
    private String valeur = "";
    private BooleanSupplier choisie = () -> false;
    private Runnable action = () -> { };

    private final InteractionListener clic = new BaseInteractionListener() {
        @Override
        public boolean onMouseClick(NUIMouseClickEvent event) {
            if (isEnabled()) {
                action.run();
            }
            return true;
        }
    };

    public Case() {
    }

    public Case(String nom, String valeur, BooleanSupplier choisie, Runnable action) {
        regler(nom, valeur, choisie, action);
    }

    /** For a row the layout file places: what it says, when it is ticked, what a click does. */
    public void regler(String texte, String droite, BooleanSupplier coche, Runnable clicAction) {
        this.nom = texte;
        this.valeur = droite == null ? "" : droite;
        this.choisie = coche;
        this.action = clicAction;
    }

    @Override
    public void onDraw(Canvas canvas) {
        Rectanglei r = canvas.getRegion();
        boolean ici = choisie.getAsBoolean();
        String uri = ici ? "engine:button" : clic.isMouseOver() ? "engine:listItemHover" : "engine:listItem";
        UITextureRegion fond = texture(uri);
        if (fond != null) {
            Border bord = ici ? new Border(9, 9, 9, 18) : new Border(12, 12, 12, 12);
            canvas.drawTextureRawBordered(fond, r, bord, true);
        }

        int y = r.minY + (r.lengthY() - CARRE) / 2 - (ici ? 2 : 0);
        int x = r.minX + 12;
        canvas.drawFilledRectangle(new Rectanglei(x, y, x + CARRE, y + CARRE), CERCLE);
        canvas.drawFilledRectangle(new Rectanglei(x + 3, y + 3, x + CARRE - 3, y + CARRE - 3), ici ? REMPLI : CREUX);

        Font police = Assets.getFont("engine:Grenze-Strong").orElse(null);
        if (police == null) {
            return;
        }
        int bas = ici ? 4 : 0;
        Rectanglei texte = new Rectanglei(x + CARRE + 10, r.minY, r.maxX - 12, r.maxY - bas);
        canvas.drawTextRawShadowed(nom, police, isEnabled() ? TEXTE : VALEUR, new Color(0x00000088), texte,
                HorizontalAlign.LEFT, VerticalAlign.MIDDLE);
        if (!valeur.isEmpty()) {
            Font petite = Assets.getFont("engine:Grenze-Body").orElse(police);
            canvas.drawTextRaw(valeur, petite, ici ? VALEUR_CHOISIE : VALEUR, texte, HorizontalAlign.RIGHT,
                    VerticalAlign.MIDDLE);
        }
        canvas.addInteractionRegion(clic);
    }

    @Override
    public Vector2i getPreferredContentSize(Canvas canvas, Vector2i sizeHint) {
        Font police = Assets.getFont("engine:Grenze-Strong").orElse(null);
        int largeur = police == null ? 160 : police.getWidth(nom) + (valeur.isEmpty() ? 0 : 60) + CARRE + 40;
        return new Vector2i(largeur, HAUTEUR);
    }

    private static UITextureRegion texture(String uri) {
        return Assets.getTextureRegion(uri).orElse(null);
    }
}
