// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

/**
 * A rectangle laid on a named piece: for marks the piece is tete, teteD, teteG, torse or bras; for hairstyle extras
 * it is a face of the layer-2 head.
 */
public record Pose(String piece, int x, int y, int largeur, int hauteur) {
}
