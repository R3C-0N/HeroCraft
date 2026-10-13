// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;

/** A sub-race: its own skin palette (null keeps the species'), proportions and traits laid over the species'. */
public record Variante(String nom, String apparence, List<String> peaux, Mods mods, Traits traits) {
}
