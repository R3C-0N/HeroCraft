// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

/**
 * A rectangle of colour on a face, named by a tint key rather than a colour: eyes use b, i, l, c (white, iris, lid,
 * lash), mouths l, f, d, p (lip, hollow, tooth, shaded skin), outfits fer, clair, sombre, base, cuir, peau.
 */
public record Touche(int x, int y, int largeur, int hauteur, String teinte) {
}
