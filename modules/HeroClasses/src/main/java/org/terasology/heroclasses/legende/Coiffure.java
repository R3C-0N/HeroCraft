// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;

/**
 * A hairstyle. The three profiles give, column by column, how many texels of hair hang down the front, the sides
 * (front to back) and the back of the layer-2 head; any may be null. {@code chute} names the hanging piece, if any.
 */
public record Coiffure(String id, String nom, int[] dev, int[] cot, int[] der, String chute, List<Pose> extras) {
}
