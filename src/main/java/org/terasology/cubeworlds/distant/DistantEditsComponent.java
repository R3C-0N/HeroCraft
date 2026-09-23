// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.terasology.gestalt.entitysystem.component.Component;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * On the world entity: the chunks players changed, as the distant terrain read them, saved with the
 * world.
 * <p>
 * A component because it is what a module may persist: the engine's chunk store is not part of the
 * module API, so the distance cannot read a saved chunk back on its own, and without this a wall
 * built last session would be gone from the distance until someone walked up to it again. Each chunk
 * is run-length encoded along its cells and written as base64; the materials are block names, since
 * palette numbers are only valid for one session.
 */
public class DistantEditsComponent implements Component<DistantEditsComponent> {

    /** Block names, in the order the encoded cells number them from one; zero is air. */
    public List<String> materials = new ArrayList<>();

    /** Chunk position "x,y,z" to its encoded cells. */
    public Map<String, String> chunks = new HashMap<>();

    @Override
    public void copyFrom(DistantEditsComponent other) {
        this.materials = new ArrayList<>(other.materials);
        this.chunks = new HashMap<>(other.chunks);
    }
}
