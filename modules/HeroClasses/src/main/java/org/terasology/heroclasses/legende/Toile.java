// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.ArrayDeque;
import java.util.Deque;

/**
 * The few canvas calls the painter makes, with the mock-up's exact semantics: source-over compositing in straight
 * alpha, in doubles, each channel rounded as it is written. Starts fully transparent.
 */
final class Toile {

    static final int TAILLE = 64;

    /** RGBA, row by row, one int per channel. */
    final int[] px = new int[TAILLE * TAILLE * 4];
    double globalAlpha = 1;
    private final Deque<Double> pile = new ArrayDeque<>();

    void save() {
        pile.push(globalAlpha);
    }

    void restore() {
        globalAlpha = pile.pop();
    }

    void fillRect(int x, int y, int w, int h, int rgb) {
        fillRect(x, y, w, h, rgb, 1);
    }

    /** Lays a colour of opacity {@code a0} (times the global alpha) over a rectangle, clipped to the sheet. */
    void fillRect(int x, int y, int w, int h, int rgb, double a0) {
        int r = rgb >> 16 & 255;
        int g = rgb >> 8 & 255;
        int b = rgb & 255;
        double a = a0 * globalAlpha;
        for (int j = Math.max(0, y); j < Math.min(TAILLE, y + h); j++) {
            for (int i = Math.max(0, x); i < Math.min(TAILLE, x + w); i++) {
                int k = (j * TAILLE + i) * 4;
                double da = px[k + 3] / 255.0;
                double oa = a + da * (1 - a);
                if (oa <= 0) {
                    px[k] = 0;
                    px[k + 1] = 0;
                    px[k + 2] = 0;
                    px[k + 3] = 0;
                    continue;
                }
                px[k] = (int) Math.round((r * a + px[k] * da * (1 - a)) / oa);
                px[k + 1] = (int) Math.round((g * a + px[k + 1] * da * (1 - a)) / oa);
                px[k + 2] = (int) Math.round((b * a + px[k + 2] * da * (1 - a)) / oa);
                px[k + 3] = (int) Math.round(oa * 255);
            }
        }
    }
}
