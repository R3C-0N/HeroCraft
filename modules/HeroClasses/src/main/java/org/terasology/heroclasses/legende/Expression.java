// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;

/** An eye or mouth shape: rectangles on the face, eyes given on the half face and mirrored. */
public record Expression(String id, String nom, List<Touche> ops) {
}
