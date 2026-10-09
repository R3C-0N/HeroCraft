// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector3f;
import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.chunks.ChunkBlockIterator;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.joml.geom.AABBfc;
import org.terasology.protobuf.EntityData;

/**
 * A chunk that is not one: it keeps nothing and hands every block set into it to a consumer.
 * <p>
 * A tree generator writes a tree into a chunk, clipped to it, and nothing else of the chunk. Given
 * this instead, with the tree's base in the middle, the whole tree comes out — a tree fits in a
 * chunk, thirteen blocks either side and thirty-five up — block by block, and the distant terrain
 * gets the very tree the game would grow there, from the very generator, rather than a likeness.
 * Everything but setting a block is refused.
 */
final class TreeRecorder implements Chunk {

    private static final String NOT_A_CHUNK = "A tree recorder only records the blocks set into it";

    /** Where a tree's base goes in the recorder: the middle, and low, since trees grow up. */
    static final int BASE_X = Chunks.SIZE_X / 2;
    static final int BASE_Y = 4;
    static final int BASE_Z = Chunks.SIZE_Z / 2;

    /** Receives a block and where it was set, relative to the tree's base. */
    @FunctionalInterface
    interface Sink {
        void block(int dx, int dy, int dz, Block block);
    }

    private Sink sink;

    void into(Sink newSink) {
        this.sink = newSink;
    }

    @Override
    public Block setBlock(int x, int y, int z, Block block) {
        sink.block(x - BASE_X, y - BASE_Y, z - BASE_Z, block);
        return block;
    }

    @Override
    public Vector3ic getPosition() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public Block getBlock(int x, int y, int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void setExtraData(int index, int x, int y, int z, int value) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int getExtraData(int index, int x, int y, int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public Vector3i getChunkWorldOffset(Vector3i pos) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int getChunkWorldOffsetX() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int getChunkWorldOffsetY() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int getChunkWorldOffsetZ() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public Vector3i chunkToWorldPosition(int x, int y, int z, Vector3i dest) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int chunkToWorldPositionX(int x) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int chunkToWorldPositionY(int y) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int chunkToWorldPositionZ(int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public BlockRegionc getRegion() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public byte getSunlight(int x, int y, int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean setSunlight(int x, int y, int z, byte amount) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public byte getSunlightRegen(int x, int y, int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean setSunlightRegen(int x, int y, int z, byte amount) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public byte getLight(int x, int y, int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean setLight(int x, int y, int z, byte amount) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public byte getWarmth(int x, int y, int z) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean setWarmth(int x, int y, int z, byte amount) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean hasWarmth() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public int getEstimatedMemoryConsumptionInBytes() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public ChunkBlockIterator getBlockIterator() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void markReady() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean isReady() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void deflate() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void deflateSunlight() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void dispose() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean isDisposed() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void prepareForReactivation() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public EntityData.ChunkStore.Builder encode() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean isDirty() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void setDirty(boolean dirty) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public Vector3f getRenderPosition() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public AABBfc getAABB() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void setMesh(ChunkMesh newMesh) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void setAnimated(boolean animated) {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean isAnimated() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public boolean hasMesh() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public ChunkMesh getMesh() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }

    @Override
    public void disposeMesh() {
        throw new UnsupportedOperationException(NOT_A_CHUNK);
    }
}
