// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0

package org.terasology.engine.world.internal;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.math.TeraMath;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.chunks.Chunks;

import java.util.function.BiPredicate;

public class ChunkViewCoreImpl implements ChunkViewCore {

    private static final Logger logger = LoggerFactory.getLogger(ChunkViewCoreImpl.class);

    private Vector3i offset = new Vector3i();
    private BlockRegion chunkRegion = new BlockRegion(BlockRegion.INVALID);
    private BlockRegion blockRegion = new BlockRegion(BlockRegion.INVALID);
    private Chunk[] chunks;

    private Vector3i chunkPower;
    private Vector3i chunkFilterSize;

    private Block defaultBlock;
    /** The chunks of the view that the world does not have, by index; null while every chunk is the world's. */
    private boolean[] outside;
    private Block outsideBlock;

    public ChunkViewCoreImpl(Chunk[] chunks, BlockRegionc chunkRegion, Vector3ic offset, Block defaultBlock) {
        this.chunkRegion.set(chunkRegion);
        this.chunks = chunks;
        this.offset.set(offset);
        setChunkSize(new Vector3i(Chunks.SIZE_X, Chunks.SIZE_Y, Chunks.SIZE_Z));
        this.defaultBlock = defaultBlock;
    }

    @Override
    public BlockRegionc getWorldRegion() {
        return blockRegion;
    }

    @Override
    public BlockRegionc getChunkRegion() {
        return chunkRegion;
    }

    @Override
    public Block getBlock(float x, float y, float z) {
        return getBlock(TeraMath.floorToInt(x + 0.5f), TeraMath.floorToInt(y + 0.5f), TeraMath.floorToInt(z + 0.5f));
    }

    @Override
    public Block getBlock(Vector3ic pos) {
        return getBlock(pos.x(), pos.y(), pos.z());
    }

    /**
     * Answers the chunks of the view that lie outside the world with the given block — the unloaded one, which
     * nothing shows a face to.
     * <p>
     * A cube net laid in a grid leaves slots that belong to no face. Their chunks exist, and hold air, so a block on
     * the edge of a face showed its side to them all the way down: at a cut seam, where the net is glued back only on
     * the sphere, that was a wall the height of the loaded box, seen edge on from the corner. Decided per chunk once,
     * never per block — a slot is whole chunks, and this view is read seven times a block by the mesher.
     * <p>
     * <b>Outside is relative to the chunk being meshed</b>, the centre of the view. A dead slot is not empty for
     * everybody: the pockets of the past are built there, and a pocket chunk that took its own slot for off the world
     * would mesh as nothing at all. So a chunk is hidden when it lies on the other side of the line from the centre —
     * a dead slot seen from a face, or a face seen from a dead slot — and never when both are on the same side.
     *
     * @param isOutside whether a column of the world, in block coordinates, is off the world
     */
    public void hideOutside(BiPredicate<Integer, Integer> isOutside, Block block) {
        boolean[] marks = null;
        int centreX = (chunkRegion.minX() + chunkRegion.maxX()) / 2;
        int centreZ = (chunkRegion.minZ() + chunkRegion.maxZ()) / 2;
        boolean centreOutside = isOutside.test(centreX * Chunks.SIZE_X, centreZ * Chunks.SIZE_Z);
        for (Vector3ic chunkPos : chunkRegion) {
            if (isOutside.test(chunkPos.x() * Chunks.SIZE_X, chunkPos.z() * Chunks.SIZE_Z) != centreOutside) {
                if (marks == null) {
                    marks = new boolean[chunks.length];
                }
                marks[(chunkPos.x() - chunkRegion.minX()) + chunkRegion.getSizeX()
                        * ((chunkPos.z() - chunkRegion.minZ()) + chunkRegion.getSizeZ() * (chunkPos.y() - chunkRegion.minY()))] = true;
            }
        }
        outside = marks;
        outsideBlock = block;
    }

    // TODO: Review
    @Override
    public Block getBlock(int blockX, int blockY, int blockZ) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            int index = relChunkIndex(blockX, blockY, blockZ);
            if (outside != null && outside[index]) {
                return outsideBlock;
            }
            Chunk chunk = chunks[index];
            if (chunk != null) {
                return chunk.getBlock(
                        Chunks.toRelative(blockX, chunkFilterSize.x),
                        Chunks.toRelative(blockY, chunkFilterSize.y),
                        Chunks.toRelative(blockZ, chunkFilterSize.z));
            }
        }
        return defaultBlock;
    }

    @Override
    public byte getSunlight(float x, float y, float z) {
        return getSunlight(TeraMath.floorToInt(x + 0.5f), TeraMath.floorToInt(y + 0.5f), TeraMath.floorToInt(z + 0.5f));
    }

    @Override
    public byte getSunlight(Vector3i pos) {
        return getSunlight(pos.x, pos.y, pos.z);
    }

    @Override
    public byte getLight(float x, float y, float z) {
        return getLight(TeraMath.floorToInt(x + 0.5f), TeraMath.floorToInt(y + 0.5f), TeraMath.floorToInt(z + 0.5f));
    }

    @Override
    public byte getLight(Vector3i pos) {
        return getLight(pos.x, pos.y, pos.z);
    }

    // The mesher reads sixteen light samples per vertex, so like getBlock these two stay on ints and build no vector.
    @Override
    public byte getSunlight(int blockX, int blockY, int blockZ) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                return chunk.getSunlight(
                        Chunks.toRelative(blockX, chunkFilterSize.x),
                        Chunks.toRelative(blockY, chunkFilterSize.y),
                        Chunks.toRelative(blockZ, chunkFilterSize.z));
            }
        }
        return 0;
    }

    @Override
    public byte getLight(int blockX, int blockY, int blockZ) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                return chunk.getLight(
                        Chunks.toRelative(blockX, chunkFilterSize.x),
                        Chunks.toRelative(blockY, chunkFilterSize.y),
                        Chunks.toRelative(blockZ, chunkFilterSize.z));
            }
        }
        return 0;
    }

    @Override
    public byte getWarmth(float x, float y, float z) {
        return getWarmth(TeraMath.floorToInt(x + 0.5f), TeraMath.floorToInt(y + 0.5f), TeraMath.floorToInt(z + 0.5f));
    }

    @Override
    public byte getWarmth(int blockX, int blockY, int blockZ) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                return chunk.getWarmth(
                        Chunks.toRelative(blockX, chunkFilterSize.x),
                        Chunks.toRelative(blockY, chunkFilterSize.y),
                        Chunks.toRelative(blockZ, chunkFilterSize.z));
            }
        }
        return 0;
    }

    @Override
    public void setBlock(Vector3ic pos, Block type) {
        setBlock(pos.x(), pos.y(), pos.z(), type);
    }

    @Override
    public void setBlock(int blockX, int blockY, int blockZ, Block type) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                chunk.setBlock(Chunks.toRelative(blockX, blockY, blockZ, chunkFilterSize, new Vector3i()), type);
                return;
            }
        }
        logger.warn("Attempt to modify block outside of the view");
    }

    @Override
    public void setLight(Vector3ic pos, byte light) {
        setLight(pos.x(), pos.y(), pos.z(), light);
    }

    @Override
    public void setLight(int blockX, int blockY, int blockZ, byte light) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                chunk.setLight(Chunks.toRelative(blockX, blockY, blockZ, chunkFilterSize, new Vector3i()), light);
                return;
            }
        }
        logger.warn("Attempted to set light at a position not encompassed by the view");
    }

    @Override
    public void setSunlight(Vector3ic pos, byte light) {
        setSunlight(pos.x(), pos.y(), pos.z(), light);
    }

    @Override
    public void setSunlight(int blockX, int blockY, int blockZ, byte light) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                chunk.setSunlight(Chunks.toRelative(blockX, blockY, blockZ, chunkFilterSize, new Vector3i()), light);
                return;
            }
        }
        throw new IllegalStateException("Attempted to set sunlight at a position not encompassed by the view");
    }

    @Override
    public int getExtraData(int index, Vector3ic pos) {
        return getExtraData(index, pos.x(), pos.y(), pos.z());
    }

    @Override
    public int getExtraData(int index, int blockX, int blockY, int blockZ) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                return chunk.getExtraData(index, Chunks.toRelative(blockX, blockY, blockZ, chunkFilterSize, new Vector3i()));
            }
        }
        return 0;
    }

    @Override
    public void setExtraData(int index, Vector3ic pos, int value) {
        setExtraData(index, pos.x(), pos.y(), pos.z(), value);
    }

    @Override
    public void setExtraData(int index, int blockX, int blockY, int blockZ, int value) {
        if (blockRegion.contains(blockX, blockY, blockZ)) {
            Chunk chunk = chunks[relChunkIndex(blockX, blockY, blockZ)];
            if (chunk != null) {
                chunk.setExtraData(index, Chunks.toRelative(blockX, blockY, blockZ, chunkFilterSize, new Vector3i()), value);
            }
        }
        throw new IllegalStateException("Attempted to modify extra data at a position not encompassed by the view");
    }

    @Override
    public void setDirtyAround(Vector3ic blockPos) {
        setDirtyAround(new BlockRegion(blockPos));
    }

    @Override
    public void setDirtyAround(BlockRegionc region) {
        BlockRegion tmp = new BlockRegion(region).expand(1, 1, 1);
        for (Vector3ic pos : Chunks.toChunkRegion(tmp, tmp)) {
            int px = pos.x() + offset.x;
            int py = pos.y() + offset.y;
            int pz = pos.z() + offset.z;
            Chunk chunk = chunks[px + chunkRegion.getSizeX() * (pz + chunkRegion.getSizeZ() * py)];
            if (chunk != null) {
                chunk.setDirty(true);
            }
        }
    }

    @Override
    public boolean isValidView() {
        for (Chunk chunk : chunks) {
            if (chunk != null && chunk.isDisposed()) {
                return false;
            }
        }
        return true;
    }

    protected int relChunkIndex(int x, int y, int z) {
        int px = (Chunks.toChunkPos(x, chunkPower.x) + offset.x);
        int py = Chunks.toChunkPos(y, chunkPower.y) + offset.y;
        int pz = Chunks.toChunkPos(z, chunkPower.z) + offset.z;
        return  px + chunkRegion.getSizeX() * (pz + chunkRegion.getSizeZ() * py);
    }

    public void setChunkSize(Vector3i chunkSize) {
        this.chunkFilterSize = new Vector3i(TeraMath.ceilPowerOfTwo(chunkSize.x) - 1,
                TeraMath.ceilPowerOfTwo(chunkSize.y) - 1,
                TeraMath.ceilPowerOfTwo(chunkSize.z) - 1);
        this.chunkPower = new Vector3i(TeraMath.sizeOfPower(chunkSize.x), TeraMath.sizeOfPower(chunkSize.y), TeraMath.sizeOfPower(chunkSize.z));

        Vector3i blockMin = new Vector3i();
        blockMin.sub(offset);
        blockMin.mul(chunkSize.x, chunkSize.y, chunkSize.z);
        Vector3i blockSize = chunkRegion.getSize(new Vector3i());
        blockSize.mul(chunkSize.x, chunkSize.y, chunkSize.z);
        this.blockRegion.setPosition(blockMin).setSize(blockSize);

    }

    @Override
    public Vector3i toWorldPos(Vector3ic localPos) {
        return new Vector3i(localPos.x() + (offset.x + chunkRegion.minX()) * Chunks.SIZE_X,
                localPos.y() + (offset.y + chunkRegion.minY()) * Chunks.SIZE_Y,
                localPos.z() + (offset.z + chunkRegion.minZ()) * Chunks.SIZE_Z);
    }
}
