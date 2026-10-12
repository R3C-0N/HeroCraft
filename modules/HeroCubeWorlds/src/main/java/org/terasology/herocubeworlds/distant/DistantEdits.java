// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.chunks.Chunk;
import org.terasology.engine.world.chunks.ChunkProvider;
import org.terasology.engine.world.chunks.Chunks;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.IntFunction;
import java.util.function.ToIntFunction;

/**
 * What players changed in the world, as the distant terrain has to see it.
 * <p>
 * The distance is summarised from the generator, which knows nothing of a wall someone built. So
 * every chunk in which a block changed is noted, and read back — block for block, in the palette's
 * numbers — in batches, a couple of times a second, while it is still loaded. A captured chunk then
 * stands in for the generator wherever the summarizer reads it, and the nodes that cover it are
 * rebuilt in the background. Nothing else is ever captured: an untouched chunk is the generator's,
 * and summarising it again from its blocks would only cost.
 * <p>
 * At a coarse level a column is one block of the world out of {@code stride²}, read where it falls:
 * a change narrower than a column can vanish in the distance, as it would from a painting at that
 * range. That is the price of summarising by sampling rather than by merging.
 */
public final class DistantEdits {

    /** No capture covers this cell: ask the generator. */
    public static final int NO_EDIT = -2;
    /** A captured cell of air. */
    public static final int AIR = -1;

    private static final int CELLS = Chunks.SIZE_X * Chunks.SIZE_Y * Chunks.SIZE_Z;

    private final Set<Long> noted = ConcurrentHashMap.newKeySet();
    private final Map<Long, short[]> captured = new ConcurrentHashMap<>();
    private final Set<Long> columns = ConcurrentHashMap.newKeySet();

    /** Called for every changed block. Cheap: a key into a set. */
    public void note(Vector3ic blockPos) {
        noted.add(key(Math.floorDiv(blockPos.x(), Chunks.SIZE_X), Math.floorDiv(blockPos.y(), Chunks.SIZE_Y),
                Math.floorDiv(blockPos.z(), Chunks.SIZE_Z)));
    }

    /**
     * Reads back the noted chunks that are loaded. Main thread: it reads the chunks.
     *
     * @return the columns of chunks that changed, packed as {@link #columnKey}
     */
    public Set<Long> capture(ChunkProvider chunks, DistantPalette palette) {
        Set<Long> changedColumns = new HashSet<>();
        if (noted.isEmpty()) {
            return changedColumns;
        }
        Vector3i pos = new Vector3i();
        for (Long key : new HashSet<>(noted)) {
            unpack(key, pos);
            Chunk chunk = chunks.getChunk(pos);
            if (chunk == null || !chunk.isReady()) {
                // Unloaded before it could be read: its changes are in the save, and it will be
                // noted again the next time a block of it changes.
                noted.remove(key);
                continue;
            }
            short[] cells = new short[CELLS];
            int i = 0;
            for (int y = 0; y < Chunks.SIZE_Y; y++) {
                for (int z = 0; z < Chunks.SIZE_Z; z++) {
                    for (int x = 0; x < Chunks.SIZE_X; x++) {
                        Block block = chunk.getBlock(x, y, z);
                        cells[i++] = (short) (block.isPenetrable() && !block.isLiquid() ? 0 : palette.idOf(block) + 1);
                    }
                }
            }
            put(key, cells);
            noted.remove(key);
            changedColumns.add(columnKey(pos.x(), pos.z()));
        }
        return changedColumns;
    }

    /** Whether any capture lies in the column of chunks under this column of the world. */
    public boolean touches(int x, int z) {
        return !columns.isEmpty()
                && columns.contains(columnKey(Math.floorDiv(x, Chunks.SIZE_X), Math.floorDiv(z, Chunks.SIZE_Z)));
    }

    /** @return a palette number, {@link #AIR}, or {@link #NO_EDIT} */
    public int cell(int x, int y, int z) {
        int cx = Math.floorDiv(x, Chunks.SIZE_X);
        int cy = Math.floorDiv(y, Chunks.SIZE_Y);
        int cz = Math.floorDiv(z, Chunks.SIZE_Z);
        short[] cells = captured.get(key(cx, cy, cz));
        if (cells == null) {
            return NO_EDIT;
        }
        int lx = x - cx * Chunks.SIZE_X;
        int ly = y - cy * Chunks.SIZE_Y;
        int lz = z - cz * Chunks.SIZE_Z;
        return cells[lx + Chunks.SIZE_X * (lz + Chunks.SIZE_Z * ly)] - 1;
    }

    /**
     * Writes every capture into the component, materials by name.
     *
     * @param nameOf the block name of a palette number
     */
    public void saveInto(DistantEditsComponent component, IntFunction<String> nameOf) {
        List<String> names = new ArrayList<>();
        Map<String, Integer> index = new HashMap<>();
        Map<String, String> chunks = new HashMap<>();
        Vector3i pos = new Vector3i();
        for (Map.Entry<Long, short[]> entry : captured.entrySet()) {
            short[] cells = entry.getValue();
            short[] renamed = new short[cells.length];
            for (int i = 0; i < cells.length; i++) {
                if (cells[i] == 0) {
                    continue;
                }
                String name = nameOf.apply(cells[i] - 1);
                Integer n = index.get(name);
                if (n == null) {
                    n = names.size() + 1;
                    names.add(name);
                    index.put(name, n);
                }
                renamed[i] = (short) (int) n;
            }
            unpack(entry.getKey(), pos);
            chunks.put(pos.x() + "," + pos.y() + "," + pos.z(), encode(renamed));
        }
        component.materials = names;
        component.chunks = chunks;
    }

    /**
     * Reads the captures back from a save.
     *
     * @param idOf the palette number of a block name, or a negative number if the block is gone
     */
    public void loadFrom(DistantEditsComponent component, ToIntFunction<String> idOf) {
        int[] ids = new int[component.materials.size() + 1];
        for (int n = 0; n < component.materials.size(); n++) {
            ids[n + 1] = idOf.applyAsInt(component.materials.get(n));
        }
        for (Map.Entry<String, String> entry : component.chunks.entrySet()) {
            String[] xyz = entry.getKey().split(",");
            short[] cells = decode(entry.getValue());
            for (int i = 0; i < cells.length; i++) {
                int id = cells[i] == 0 ? -1 : ids[cells[i]];
                cells[i] = (short) (id < 0 ? 0 : id + 1);
            }
            put(key(Integer.parseInt(xyz[0]), Integer.parseInt(xyz[1]), Integer.parseInt(xyz[2])), cells);
        }
    }

    /** Runs of equal cells along the chunk's own order, as (length, value) pairs of shorts. */
    static String encode(short[] cells) {
        ByteBuffer out = ByteBuffer.allocate(4 * cells.length);
        int i = 0;
        while (i < cells.length) {
            short value = cells[i];
            int run = 1;
            while (i + run < cells.length && cells[i + run] == value && run < Short.MAX_VALUE) {
                run++;
            }
            out.putShort((short) run).putShort(value);
            i += run;
        }
        byte[] bytes = new byte[out.position()];
        out.flip();
        out.get(bytes);
        return Base64.getEncoder().encodeToString(bytes);
    }

    static short[] decode(String text) {
        ByteBuffer in = ByteBuffer.wrap(Base64.getDecoder().decode(text));
        short[] cells = new short[CELLS];
        int i = 0;
        while (in.remaining() >= 4 && i < cells.length) {
            int run = in.getShort();
            short value = in.getShort();
            for (int k = 0; k < run && i < cells.length; k++) {
                cells[i++] = value;
            }
        }
        return cells;
    }

    void put(long key, short[] cells) {
        captured.put(key, cells);
        Vector3i pos = unpack(key, new Vector3i());
        columns.add(columnKey(pos.x(), pos.z()));
    }

    public int capturedChunks() {
        return captured.size();
    }

    static long key(int cx, int cy, int cz) {
        return ((long) (cx & 0x1FFFFF) << 42) | ((long) (cy & 0x1FFFFF) << 21) | (cz & 0x1FFFFF);
    }

    static Vector3i unpack(long key, Vector3i dest) {
        return dest.set(signed((int) (key >>> 42)), signed((int) (key >>> 21)), signed((int) key));
    }

    static long columnKey(int cx, int cz) {
        return ((long) cx << 32) | (cz & 0xFFFFFFFFL);
    }

    private static int signed(int v) {
        int w = v & 0x1FFFFF;
        return (w & 0x100000) != 0 ? w - 0x200000 : w;
    }
}
