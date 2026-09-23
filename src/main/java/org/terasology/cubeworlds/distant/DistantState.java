// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.terasology.engine.rendering.world.DistantReach;

/**
 * What the distant terrain pass reads each frame, put into the context once and emptied on shutdown.
 * <p>
 * A holder and not the objects themselves: the context has no way to take an entry back out, and
 * putting null into it throws — which is what failed the clean shutdown of the game and left a JVM
 * behind with no main thread.
 */
public final class DistantState implements DistantReach {

    private volatile DistantTerrain terrain;
    private volatile DistantHole hole;
    private volatile DistantTerrainNode node;
    private volatile float reach;
    private volatile float frontier;

    public DistantTerrain terrain() {
        return terrain;
    }

    public DistantHole hole() {
        return hole;
    }

    /** The pass, which registers itself: it lives in the rendering context, the system does not. */
    public DistantTerrainNode node() {
        return node;
    }

    void setNode(DistantTerrainNode pass) {
        this.node = pass;
    }

    void set(DistantTerrain newTerrain, DistantHole newHole) {
        this.terrain = newTerrain;
        this.hole = newHole;
        if (newTerrain == null) {
            reach = 0;
        }
    }

    void setReach(float newReach, float newFrontier) {
        this.reach = newReach;
        this.frontier = newFrontier;
    }

    @Override
    public float reach() {
        return terrain == null ? 0 : reach;
    }

    @Override
    public float frontier() {
        return frontier;
    }
}
