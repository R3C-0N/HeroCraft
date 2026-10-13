// Copyright 2022 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0

package org.terasology.engine.core.modes.loadProcesses;

import org.terasology.engine.context.Context;
import org.terasology.engine.core.EngineTime;
import org.terasology.engine.core.Time;
import org.terasology.engine.core.modes.VariableStepLoadProcess;
import org.terasology.engine.rendering.world.WorldRenderer;

/**
 * Loops until the chunks round the player are meshed, or a minute has elapsed.
 * <p>
 * Five seconds used to be the limit, and a world slower to generate than that opened on whatever stood in for
 * the ground: the distant terrain of a cube world, coarse and at the wrong height, right under the player. The
 * minute is only there so that a world that never finishes cannot hold the loading screen for ever.
 */
public class PrepareWorld extends VariableStepLoadProcess {

    public static int maximumWaitMs = 60000;

    private final Context context;
    private long startTime;
    private WorldRenderer worldRenderer;
    private long timeElapsed;

    public PrepareWorld(Context context) {
        this.context = context;
    }

    @Override
    public String getMessage() {
        return "${engine:menu#catching-world}";
    }

    @Override
    public boolean step() {
        if (worldRenderer.pregenerateChunks()) {
            return true;
        }
        Thread.onSpinWait();
        EngineTime time = (EngineTime) context.get(Time.class);
        timeElapsed = time.getRealTimeInMs() - startTime;
        return timeElapsed > maximumWaitMs;
    }

    @Override
    public void begin() {
        worldRenderer = context.get(WorldRenderer.class);
        EngineTime time = (EngineTime) context.get(Time.class);
        startTime = time.getRealTimeInMs();
    }

    @Override
    public float getProgress() {
        return ((float) timeElapsed) / maximumWaitMs;
    }

    @Override
    public int getExpectedCost() {
        return 5;
    }

}
