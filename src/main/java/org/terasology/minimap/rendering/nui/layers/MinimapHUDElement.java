// Copyright 2020 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.minimap.rendering.nui.layers;

import org.joml.Vector3ic;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.rendering.nui.layers.hud.CoreHudWidget;
import org.terasology.engine.world.WorldProvider;
import org.terasology.minimap.overlays.MinimapOverlay;
import org.terasology.nui.databinding.ReadOnlyBinding;

/**
 * The minimap container: the map in its HUD frame. Zoom has no widget of its
 * own; it follows the zoom key bindings.
 */
public class MinimapHUDElement extends CoreHudWidget {

    private static final String MINIMAP_GRID_WIDGET_ID = "minimapGrid";
    private static final int MIN_ZOOM = -10;
    private static final int MAX_ZOOM = 10;

    private MinimapGrid minimapGrid;
    private int zoomLevel;

    @Override
    public void initialise() {
        minimapGrid = find(MINIMAP_GRID_WIDGET_ID, MinimapGrid.class);

        minimapGrid.bindZoomFactor(new ReadOnlyBinding<Integer>() {
            @Override
            public Integer get() {
                return zoomLevel;
            }
        });
    }

    public void changeZoom(int delta) {
        zoomLevel = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoomLevel + delta));
    }

    public void updateLocation(Vector3ic worldLocation) {
        minimapGrid.updateLocation(worldLocation);
    }

    public EntityRef getTargetEntity() {
        return minimapGrid.getTargetEntity();
    }

    public void setTargetEntity(EntityRef characterEntity) {
        minimapGrid.setTargetEntity(characterEntity);
    }

    public void bindTargetEntity(ReadOnlyBinding<EntityRef> binding) {
        minimapGrid.bindTargetEntity(binding);
    }

    public void updateAlivePlayerList(Iterable<EntityRef> alivePlayersIterable) {
        minimapGrid.updateAlivePlayerList(alivePlayersIterable);
    }

    public void addAlivePlayer(EntityRef player) {
        minimapGrid.addAlivePlayer(player);
    }

    public void removeAlivePlayer(EntityRef player) {
        minimapGrid.removeAlivePlayer(player);
    }

    public void setHeightRange(int bottom, int top) {
        minimapGrid.setHeightRange(bottom, top);
    }

    public void setWorldProvider(WorldProvider worldProvider) {
        minimapGrid.setWorldProvider(worldProvider);
    }

    /**
     * @param overlay the overlay to add
     */
    public void addOverlay(MinimapOverlay overlay) {
        minimapGrid.addOverlay(overlay);
    }

    /**
     * @param overlay the overlay to remove
     */
    public void removeOverlay(MinimapOverlay overlay) {
        minimapGrid.removeOverlay(overlay);
    }
}
