// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;

/**
 * An outfit. {@code ton} picks the body colour from the chosen cloth (base, sombre, clair, plaque); {@code manche}
 * is the number of the arm's twelve rows the sleeve covers; {@code jambes} is pantalon or robe.
 */
public record Tenue(String id, String nom, String ton, int manche, String jambes, boolean col, boolean ceinture,
                    boolean epaulettes, boolean torseNu, List<Touche> extras) {
}
