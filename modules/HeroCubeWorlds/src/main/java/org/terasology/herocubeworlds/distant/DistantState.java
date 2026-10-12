// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.rendering.world.DistantGeometry;
import org.terasology.engine.rendering.world.DistantReach;

/**
 * What the passes of the renderer read of the distant terrain each frame, put into the context once
 * and emptied on shutdown: how far it reaches, for the haze, and the terrain itself, for the passes
 * that draw the chunks and then draw it.
 * <p>
 * A holder and not the objects themselves: the context has no way to take an entry back out, and
 * putting null into it throws — which is what failed the clean shutdown of the game and left a JVM
 * behind with no main thread.
 */
public final class DistantState implements DistantReach, DistantGeometry {

    private volatile DistantTerrain terrain;
    private volatile DistantDrawer drawer;
    private volatile float reach;
    private volatile float frontier;
    private volatile boolean hidden;

    public DistantTerrain terrain() {
        return terrain;
    }

    public DistantDrawer drawer() {
        return drawer;
    }

    void set(DistantTerrain newTerrain, DistantDrawer newDrawer) {
        this.terrain = newTerrain;
        this.drawer = newDrawer;
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

    /** Leaves the distance out of every pass, to tell what it draws from what the chunks draw. */
    void setHidden(boolean newHidden) {
        this.hidden = newHidden;
    }

    boolean isHidden() {
        return hidden;
    }

    @Override
    public int render(ChunkMesh.RenderPhase phase, Material material, Camera camera, boolean shadow) {
        DistantDrawer current = drawer;
        return current == null || hidden ? 0 : current.render(phase, material, camera, shadow);
    }
}
