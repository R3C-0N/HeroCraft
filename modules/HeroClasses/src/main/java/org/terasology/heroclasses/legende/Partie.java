// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.Map;

/**
 * One box of the pattern: the bone it follows, its layer, its size in texels, how much layer 2 inflates it, and
 * where each of its six faces lies on the sheet.
 */
public record Partie(String id, String os, int couche, int largeur, int hauteur, int profondeur, double gonflement,
                     Map<String, Rect> faces) {

    /** @return the rectangle of a face: dessus, dessous, droite, devant, gauche or derriere */
    public Rect face(String face) {
        return faces.get(face);
    }
}
