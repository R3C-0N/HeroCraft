// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.engine.world.block.tiles;

import com.google.common.collect.Lists;
import org.terasology.gestalt.assets.Asset;
import org.terasology.gestalt.assets.AssetType;
import org.terasology.gestalt.assets.ResourceUrn;
import org.terasology.context.annotation.API;

import java.awt.image.BufferedImage;
import java.util.Collections;
import java.util.List;
import java.util.function.Consumer;

@API
public class BlockTile extends Asset<TileData> {
    private BufferedImage[] images;
    private boolean autoBlock;
    private List<Consumer<BlockTile>> reloadListeners = Collections.synchronizedList(Lists.newArrayList());

    public BlockTile(ResourceUrn urn, AssetType<?, TileData> assetType, TileData data) {
        super(urn, assetType);
        reload(data);
    }

    public BufferedImage getImage() {
        return getImage(0);
    }

    public BufferedImage getImage(int i) {
        return images[i];
    }

    /**
     * The mean colour of the first image, its clear texels left out, as {red, green, blue} from 0 to 1:
     * what the tile reads as from afar. Grey 0.5 for a tile with nothing opaque.
     */
    public float[] getMeanColour() {
        BufferedImage image = images[0];
        double red = 0;
        double green = 0;
        double blue = 0;
        int opaque = 0;
        for (int y = 0; y < image.getHeight(); y++) {
            for (int x = 0; x < image.getWidth(); x++) {
                int argb = image.getRGB(x, y);
                if ((argb >>> 24) < 26) {
                    continue;
                }
                red += (argb >> 16) & 0xFF;
                green += (argb >> 8) & 0xFF;
                blue += argb & 0xFF;
                opaque++;
            }
        }
        if (opaque == 0) {
            return new float[]{0.5f, 0.5f, 0.5f};
        }
        return new float[]{(float) (red / opaque / 255), (float) (green / opaque / 255), (float) (blue / opaque / 255)};
    }

    public int getLength() {
        return images.length;
    }

    public boolean isAutoBlock() {
        return autoBlock;
    }

    public synchronized void subscribe(Consumer<BlockTile> reloadListener) {
        this.reloadListeners.add(reloadListener);
    }

    public synchronized void unsubscribe(Consumer<BlockTile> reloadListener) {
        this.reloadListeners.remove(reloadListener);
    }

    @Override
    protected void doReload(TileData tileData) {
        this.images = tileData.getImages();
        this.autoBlock = tileData.isAutoBlock();
        for (Consumer<BlockTile> listener : reloadListeners) {
            listener.accept(this);
        }
    }

}
