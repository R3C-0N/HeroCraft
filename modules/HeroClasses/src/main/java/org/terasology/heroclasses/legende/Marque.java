// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;

/** A body mark. {@code teinte} is a colour, or {@code peauF} for a darker shade of the skin; null for none. */
public record Marque(String id, String nom, String teinte, List<Pose> ops) {
}
