// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.terasology.engine.context.Context;
import org.terasology.engine.rendering.dag.ModuleRendering;
import org.terasology.engine.rendering.dag.Node;

/**
 * Puts the distant terrain pass into the render graph, next to the opaque blocks.
 * <p>
 * After the two rendering modules of CoreRendering, whose nodes it reads by name: it draws into the
 * buffers the opaque blocks draw into — behind the haze, whose nodes run first so that only the sky
 * feeds it — and before the deferred main light, which then lights the distance as it lights the
 * terrain. Present on every world with this module, and idle on any world without a distant terrain
 * in its context.
 */
public class DistantRenderingModule extends ModuleRendering {

    private static final int INITIALIZATION_PRIORITY = 3;
    /** The run-order inputs 1 to 3 of the main light are CoreRendering's. */
    private static final int MAIN_LIGHT_INPUT = 4;

    public DistantRenderingModule(Context context) {
        super(context);
        setInitializationPriority(INITIALIZATION_PRIORITY);
    }

    @Override
    public void initialise() {
        super.initialise();
        Node source = renderGraph.findAka("finalHaze");
        if (source == null) {
            source = renderGraph.findAka("backdrop");
        }
        if (source == null) {
            logger.warn("No haze nor backdrop node: the distant terrain has nowhere to draw.");
            return;
        }
        DistantTerrainNode node = new DistantTerrainNode("distantTerrainNode", providingModule, context);
        renderGraph.connectBufferPair(source, 1, node, 1);
        renderGraph.addNode(node);
        Node mainLight = renderGraph.findAka("deferredMainLight");
        if (mainLight != null) {
            renderGraph.connectRunOrder(node, 1, mainLight, MAIN_LIGHT_INPUT);
        }
    }
}
