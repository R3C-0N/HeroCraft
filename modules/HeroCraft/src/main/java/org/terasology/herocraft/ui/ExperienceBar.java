// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui;

import org.joml.Vector2i;
import org.terasology.engine.utilities.Assets;
import org.terasology.joml.geom.Rectanglei;
import org.terasology.nui.Canvas;
import org.terasology.nui.Color;
import org.terasology.nui.CoreWidget;
import org.terasology.nui.HorizontalAlign;
import org.terasology.nui.VerticalAlign;
import org.terasology.nui.asset.font.Font;
import org.terasology.nui.databinding.Binding;
import org.terasology.nui.databinding.DefaultBinding;

/**
 * The thin gold rule above the toolbar: how far the current level has gone, with the level written at its left
 * end. It shows the share of the level, not of the total — on a curve without a top, the total would fill a
 * bar that never moves again.
 */
public class ExperienceBar extends CoreWidget {
    private static final Color TRACK = new Color(0x20120ACC);
    private static final Color FILL = new Color(0xE0B24AFF);
    private static final Color TEXT = new Color(0xFFF3DCFF);
    private static final Color SHADOW = new Color(0x20120AFF);
    private static final int BAR_HEIGHT = 6;
    private static final int GAP = 2;

    private Binding<Integer> level = new DefaultBinding<>(1);
    /** Share of the current level already earned, from 0 to 1. */
    private Binding<Float> progress = new DefaultBinding<>(0f);

    public void bindLevel(Binding<Integer> binding) {
        level = binding;
    }

    public void bindProgress(Binding<Float> binding) {
        progress = binding;
    }

    @Override
    public void onDraw(Canvas canvas) {
        Rectanglei region = canvas.getRegion();
        Rectanglei bar = new Rectanglei(region.minX, region.maxY - BAR_HEIGHT, region.maxX, region.maxY);
        canvas.drawFilledRectangle(bar, TRACK);
        float share = Math.max(0f, Math.min(1f, progress.get()));
        int filled = Math.round(bar.lengthX() * share);
        if (filled > 0) {
            canvas.drawFilledRectangle(new Rectanglei(bar.minX, bar.minY, bar.minX + filled, bar.maxY), FILL);
        }
        canvas.drawTextRawShadowed("Niveau " + level.get(), font(), TEXT, SHADOW,
                new Rectanglei(region.minX, region.minY, region.maxX, bar.minY - GAP),
                HorizontalAlign.LEFT, VerticalAlign.BOTTOM);
    }

    @Override
    public Vector2i getPreferredContentSize(Canvas canvas, Vector2i sizeHint) {
        return new Vector2i(sizeHint.x, font().getLineHeight() + GAP + BAR_HEIGHT);
    }

    private static Font font() {
        return Assets.getFont("engine:Grenze-Small")
                .orElseThrow(() -> new IllegalStateException("Missing interface font engine:Grenze-Small"));
    }
}
