// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Vector2i;
import org.terasology.joml.geom.Rectanglei;
import org.terasology.nui.BaseInteractionListener;
import org.terasology.nui.Canvas;
import org.terasology.nui.Color;
import org.terasology.nui.CoreWidget;
import org.terasology.nui.InteractionListener;
import org.terasology.nui.events.NUIMouseClickEvent;

import java.util.function.BooleanSupplier;

/** One colour of a palette, to click: a square of it, ringed in gold when it is the one worn. */
public class Pastille extends CoreWidget {

    private static final int COTE = 28;
    private static final Color CADRE = new Color(0x20120AFF);
    private static final Color ANNEAU = new Color(0xE0B24AFF);

    private final Color couleur;
    private final BooleanSupplier choisie;
    private final Runnable action;

    private final InteractionListener clic = new BaseInteractionListener() {
        @Override
        public boolean onMouseClick(NUIMouseClickEvent event) {
            action.run();
            return true;
        }
    };

    /** @param hex a palette entry of the atlas, {@code #rrggbb} */
    public Pastille(String hex, BooleanSupplier choisie, Runnable action) {
        this.couleur = new Color((Integer.parseInt(hex.substring(1), 16) << 8) | 0xFF);
        this.choisie = choisie;
        this.action = action;
        setTooltip(hex.toUpperCase());
    }

    @Override
    public void onDraw(Canvas canvas) {
        Rectanglei r = canvas.getRegion();
        boolean ici = choisie.getAsBoolean();
        canvas.drawFilledRectangle(r, ici ? ANNEAU : CADRE);
        int bord = ici ? 3 : 1;
        canvas.drawFilledRectangle(new Rectanglei(r.minX + bord, r.minY + bord, r.maxX - bord, r.maxY - bord), couleur);
        canvas.addInteractionRegion(clic);
    }

    @Override
    public Vector2i getPreferredContentSize(Canvas canvas, Vector2i sizeHint) {
        return new Vector2i(COTE, COTE);
    }
}
