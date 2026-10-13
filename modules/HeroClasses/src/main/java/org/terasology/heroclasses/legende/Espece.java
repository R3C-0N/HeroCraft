// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;

/** A playable species: height range in metres, skin palette, base proportions and traits, and its sub-races. */
public record Espece(String id, String nom, double taille, double min, double max, String apparence,
                     String attribut, List<String> peaux, Mods mods, Traits traits, List<Variante> variantes) {
}
