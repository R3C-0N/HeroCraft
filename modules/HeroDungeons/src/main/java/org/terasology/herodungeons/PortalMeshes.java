// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.engine.core.GameScheduler;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.rendering.primitives.ChunkTessellator;
import org.terasology.engine.world.ChunkView;
import org.terasology.engine.world.WorldChangeListener;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.chunks.Chunks;
import reactor.core.publisher.Mono;

import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Future;

/**
 * The meshes of the chunks seen through a window: the other side, a thousand blocks or more from the eye, which the
 * renderer around the camera never meshes.
 * <p>
 * A handful of chunks, meshed once on the engine's worker threads and kept until the window looks elsewhere. A chunk is
 * meshed again only when one of its blocks changes, as the world tells its listeners; it never touches the chunk's
 * own mesh or its dirty flag, which belong to the renderer and are what it decides by when the eye arrives there.
 */
final class PortalMeshes implements WorldChangeListener {

    private static final Logger logger = LoggerFactory.getLogger(PortalMeshes.class);

    /** Meshes handed to the thread per frame, so that arriving at a window never stalls the frame on a queue. */
    private static final int SUBMITTED_PER_FRAME = 2;

    /** How long a chunk whose neighbours were not there waits before it is tried again. */
    private static final long RETRY_MS = 1000;

    private final WorldProvider world;
    private final ChunkTessellator tessellator = new ChunkTessellator();

    private final Map<Vector3ic, ChunkMesh> meshes = new HashMap<>();
    private final Map<Vector3ic, Future<ChunkMesh>> pending = new HashMap<>();
    private final Map<Vector3ic, Long> missing = new HashMap<>();
    private final Set<Vector3ic> stale = ConcurrentHashMap.newKeySet();
    private Set<Vector3ic> wanted = new HashSet<>();

    PortalMeshes(WorldProvider world) {
        this.world = world;
        world.registerListener(this);
    }

    /** The chunks to keep meshed; whatever else is held is let go. */
    void want(Collection<? extends Vector3ic> chunks) {
        Set<Vector3ic> next = new HashSet<>();
        for (Vector3ic chunk : chunks) {
            next.add(new Vector3i(chunk));
        }
        if (next.equals(wanted)) {
            return;
        }
        Iterator<Map.Entry<Vector3ic, ChunkMesh>> held = meshes.entrySet().iterator();
        while (held.hasNext()) {
            Map.Entry<Vector3ic, ChunkMesh> entry = held.next();
            if (!next.contains(entry.getKey())) {
                entry.getValue().dispose();
                held.remove();
            }
        }
        Iterator<Map.Entry<Vector3ic, Future<ChunkMesh>>> queued = pending.entrySet().iterator();
        while (queued.hasNext()) {
            Map.Entry<Vector3ic, Future<ChunkMesh>> entry = queued.next();
            if (!next.contains(entry.getKey())) {
                entry.getValue().cancel(false);
                queued.remove();
            }
        }
        missing.keySet().retainAll(next);
        wanted = next;
    }

    /** Takes in what the thread finished, and hands it the next chunks. On the thread holding the GL context. */
    void update(long now) {
        Iterator<Map.Entry<Vector3ic, Future<ChunkMesh>>> running = pending.entrySet().iterator();
        while (running.hasNext()) {
            Map.Entry<Vector3ic, Future<ChunkMesh>> entry = running.next();
            if (!entry.getValue().isDone()) {
                continue;
            }
            running.remove();
            ChunkMesh mesh = result(entry.getValue());
            if (mesh == null) {
                missing.put(entry.getKey(), now);
                continue;
            }
            ChunkMesh previous = meshes.get(entry.getKey());
            mesh.updateMesh(previous);
            mesh.discardData();
            if (previous != null && previous != mesh) {
                previous.dispose();
            }
            meshes.put(entry.getKey(), mesh);
        }
        int submitted = 0;
        for (Vector3ic chunk : wanted) {
            if (submitted >= SUBMITTED_PER_FRAME) {
                break;
            }
            boolean due = !meshes.containsKey(chunk) || stale.contains(chunk);
            Long failed = missing.get(chunk);
            if (!due || pending.containsKey(chunk) || failed != null && now - failed < RETRY_MS) {
                continue;
            }
            stale.remove(chunk);
            missing.remove(chunk);
            Vector3ic position = new Vector3i(chunk);
            pending.put(position, Mono.fromCallable(() -> mesh(position)).subscribeOn(GameScheduler.parallel()).toFuture());
            submitted++;
        }
    }

    /** The meshes ready to draw, by chunk position. */
    Map<Vector3ic, ChunkMesh> ready() {
        return meshes;
    }

    void dispose() {
        world.unregisterListener(this);
        pending.values().forEach(future -> future.cancel(false));
        meshes.values().forEach(ChunkMesh::dispose);
        meshes.clear();
        pending.clear();
    }

    @Override
    public void onBlockChanged(Vector3ic pos, Block newBlock, Block originalBlock) {
        // A block on a chunk's edge shows or hides a face of its neighbour: the six chunks round it are redone too.
        Vector3i chunk = Chunks.toChunkPos(pos, new Vector3i());
        stale.add(new Vector3i(chunk));
        stale.add(new Vector3i(chunk).add(1, 0, 0));
        stale.add(new Vector3i(chunk).add(-1, 0, 0));
        stale.add(new Vector3i(chunk).add(0, 1, 0));
        stale.add(new Vector3i(chunk).add(0, -1, 0));
        stale.add(new Vector3i(chunk).add(0, 0, 1));
        stale.add(new Vector3i(chunk).add(0, 0, -1));
        stale.retainAll(wanted);
    }

    @Override
    public void onExtraDataChanged(int i, Vector3ic pos, int newData, int oldData) {
    }

    private ChunkMesh mesh(Vector3ic chunk) {
        ChunkView view = world.getLocalView(chunk);
        if (view == null || !view.isValidView()) {
            return null;
        }
        return tessellator.generateMesh(view);
    }

    private static ChunkMesh result(Future<ChunkMesh> future) {
        try {
            return future.get();
        } catch (Exception e) {
            logger.warn("A chunk seen through a window could not be meshed", e);
            return null;
        }
    }
}
