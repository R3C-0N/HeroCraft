// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FacePos;
import org.terasology.engine.core.GameScheduler;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.world.chunks.Chunks;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Queue;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The distant terrain of one world: which nodes exist, which are shown, and the work in between.
 * <p>
 * Everything slow runs off the main thread — summarising a node from the generator, then meshing
 * it — on the engine's own parallel scheduler, a few nodes at a time, coarse and near first; the main
 * thread only chooses, uploads a few finished meshes per frame, and hands the list to the renderer.
 * A module may not start, stop or interrupt a thread of its own: the sandbox refuses it, so the work
 * goes through {@link GameScheduler} and is abandoned, not cancelled, when the terrain is disposed. A node is shown as soon as it is built; until then, the nearest built ancestor stands
 * in for it, so the distance fills coarse first and sharpens, and is never a hole.
 */
public final class DistantTerrain {

    private static final Logger logger = LoggerFactory.getLogger(DistantTerrain.class);

    /** Meshes handed to the GPU per frame: each is a buffer upload on the main thread. */
    private static final int UPLOADS_PER_FRAME = 4;

    /** A node out of the selection for this long is dropped, mesh and columns. */
    private static final long KEEP_NANOS = 20_000_000_000L;

    /** A built node, ready to draw once its mesh is on the GPU. */
    public static final class Node {
        private final SectionKey key;
        private final ColumnSection columns;
        private final int gridX;
        private final int gridZ;
        private final Vector3f renderPosition;
        private final int lowest;
        private final int highest;
        private ChunkMesh mesh;
        private boolean uploaded;
        private long lastWanted;
        /** The node this one rebuilds, drawn until this one is on the GPU. */
        private Node previous;

        Node(SectionKey key, ColumnSection columns, int[] origin, ChunkMesh mesh, int lowest, int highest) {
            this.key = key;
            this.columns = columns;
            this.gridX = origin[0];
            this.gridZ = origin[1];
            // The engine centres a block on its coordinates, and the mesh has it in the unit cell above
            // them: half a block back on every axis, and a distant vertex falls on a vertex of a block.
            this.renderPosition = new Vector3f(origin[0] - 0.5f, -0.5f, origin[1] - 0.5f);
            this.mesh = mesh;
            this.lowest = lowest;
            this.highest = highest;
        }

        public SectionKey key() {
            return key;
        }

        public ChunkMesh mesh() {
            return mesh;
        }

        /** Where the mesh is drawn from: its origin on the net, in the engine's frame of blocks. */
        public Vector3fc renderPosition() {
            return renderPosition;
        }

        /** The net column of the node's first column. */
        public int gridX() {
            return gridX;
        }

        public int gridZ() {
            return gridZ;
        }

        public int lowest() {
            return lowest;
        }

        public int highest() {
            return highest;
        }

        public boolean isDrawable() {
            return uploaded && mesh != null;
        }
    }

    private final CubeGeometry geometry;
    private final ColumnSummarizer summarizer;
    private final DistantMesher mesher;
    private final DistantSelection selection;
    private final int maxInFlight;
    private final AtomicInteger inFlight = new AtomicInteger();
    private final List<SectionKey> missing = new ArrayList<>();
    private volatile boolean disposed;
    private double lastEyeX;
    private double lastEyeZ;

    private final Map<SectionKey, Node> nodes = new ConcurrentHashMap<>();
    private final Set<SectionKey> pending = ConcurrentHashMap.newKeySet();
    private final Set<SectionKey> stale = ConcurrentHashMap.newKeySet();
    private final Queue<Node> built = new ConcurrentLinkedQueue<>();
    private volatile List<Node> shown = Collections.emptyList();

    private final AtomicInteger buildCount = new AtomicInteger();
    private final AtomicLong buildNanos = new AtomicLong();

    public DistantTerrain(CubeGeometry geometry, ColumnSummarizer summarizer, DistantMesher mesher,
                          int finestLevel, int maxInFlight) {
        this.geometry = geometry;
        this.summarizer = summarizer;
        this.mesher = mesher;
        this.selection = new DistantSelection(geometry, finestLevel);
        this.maxInFlight = maxInFlight;
    }

    /**
     * Chooses what stands for the world from this eye, asks for what is missing, and uploads what
     * has arrived. Main thread only: the uploads need the GL context.
     *
     * @param reach how far along the surface anything is kept, in blocks
     * @param sharpFrom where the band drawn at level 0 begins, along the surface
     * @param sharpTo where it ends
     */
    public void update(Vector3fc eye, double reach, double sharpFrom, double sharpTo) {
        long now = System.nanoTime();
        for (int i = 0; i < UPLOADS_PER_FRAME; i++) {
            Node node = built.poll();
            if (node == null) {
                break;
            }
            node.mesh.updateMesh();
            node.mesh.discardData();
            node.uploaded = true;
            if (node.previous != null) {
                if (node.previous.mesh != null) {
                    node.previous.mesh.dispose();
                    node.previous.mesh = null;
                }
                node.previous = null;
            }
        }

        lastEyeX = eye.x();
        lastEyeZ = eye.z();
        missing.clear();
        List<SectionKey> wanted = selection.select(eye.x(), eye.z(), reach, sharpFrom, sharpTo);
        List<Node> next = new ArrayList<>(wanted.size());
        Set<SectionKey> standIns = new HashSet<>();
        for (SectionKey key : wanted) {
            Node node = nodes.get(key);
            if (node != null) {
                node.lastWanted = now;
            }
            if (node != null && node.isDrawable()) {
                next.add(node);
                continue;
            }
            if (node != null && node.previous != null && node.previous.isDrawable()) {
                // Being rebuilt: the old one holds the ground until the new one is uploaded.
                next.add(node.previous);
                continue;
            }
            request(key);
            // Until it is built, the nearest built ancestor covers its ground.
            for (SectionKey up = parentOf(key); up != null; up = parentOf(up)) {
                Node ancestor = nodes.get(up);
                if (ancestor != null && ancestor.isDrawable()) {
                    ancestor.lastWanted = now;
                    if (standIns.add(up)) {
                        next.add(ancestor);
                    }
                    break;
                }
                request(up);
            }
        }
        shown = next;
        dispatch();
        evict(now);
    }

    /** What to draw this frame. */
    public List<Node> shown() {
        return shown;
    }

    /** Nodes built so far, and the mean cost of building one, for the bench. */
    /**
     * Marks the columns of chunks of the net that a drawable node shown now covers, on a grid {@code side}
     * columns wide: where the distance can take over from the chunks along their edge.
     */
    public void coverage(boolean[] columns, int side) {
        java.util.Arrays.fill(columns, false);
        for (Node node : shown) {
            if (!node.isDrawable()) {
                continue;
            }
            int span = node.key.span(geometry);
            int x1 = Math.min(side - 1, Math.floorDiv(node.gridX + span - 1, Chunks.SIZE_X));
            int z1 = Math.min(side - 1, Math.floorDiv(node.gridZ + span - 1, Chunks.SIZE_Z));
            for (int z = Math.max(0, Math.floorDiv(node.gridZ, Chunks.SIZE_Z)); z <= z1; z++) {
                for (int x = Math.max(0, Math.floorDiv(node.gridX, Chunks.SIZE_X)); x <= x1; x++) {
                    columns[x + z * side] = true;
                }
            }
        }
    }

    public String stats() {
        int count = buildCount.get();
        long bytes = 0;
        for (Node node : nodes.values()) {
            bytes += node.columns.bytes();
        }
        // How many of the shown nodes stand at each level, finest first: where the band and the coarsening fall.
        int[] levels = new int[SectionKey.TOP_LEVEL + 1];
        for (Node node : shown) {
            levels[node.key.level()]++;
        }
        StringBuilder byLevel = new StringBuilder();
        for (int level = 0; level < levels.length; level++) {
            byLevel.append(level == 0 ? "" : ",").append(levels[level]);
        }
        return String.format(java.util.Locale.ROOT,
                "distant nodes=%d shown=%d pending=%d stale=%d built=%d build_ms_mean=%.1f columns_mb=%.1f levels=%s",
                nodes.size(), shown.size(), pending.size(), stale.size(), count,
                count == 0 ? 0.0 : buildNanos.get() / 1e6 / count, bytes / 1048576.0, byLevel);
    }

    public void dispose() {
        disposed = true;
        for (Node node : nodes.values()) {
            if (node.mesh != null) {
                node.mesh.dispose();
            }
        }
        nodes.clear();
    }

    private void request(SectionKey key) {
        if (!nodes.containsKey(key) && !pending.contains(key) && !missing.contains(key)) {
            missing.add(key);
        }
    }

    /**
     * Marks for rebuilding every built node over these columns of chunks. The old node stays shown
     * until its replacement is uploaded, so an edit never opens a hole in the distance.
     *
     * @param chunkColumns packed as {@link DistantEdits#columnKey}
     */
    public void invalidate(Set<Long> chunkColumns) {
        if (chunkColumns.isEmpty()) {
            return;
        }
        for (Node node : nodes.values()) {
            int span = node.key.span(geometry);
            int x0 = node.gridX;
            int z0 = node.gridZ;
            for (long column : chunkColumns) {
                int cx = (int) (column >> 32) * Chunks.SIZE_X;
                int cz = (int) column * Chunks.SIZE_Z;
                if (cx < x0 + span && cx + Chunks.SIZE_X > x0
                        && cz < z0 + span && cz + Chunks.SIZE_Z > z0) {
                    stale.add(node.key);
                    break;
                }
            }
        }
    }

    /**
     * Rebuilds first, then what is missing — coarse before fine, and near before far, so that the
     * distance fills in the order it is seen.
     */
    private void dispatch() {
        for (SectionKey key : new ArrayList<>(stale)) {
            if (inFlight.get() >= maxInFlight) {
                return;
            }
            if (pending.add(key)) {
                stale.remove(key);
                inFlight.incrementAndGet();
                GameScheduler.scheduleParallel("Distant terrain", () -> build(key));
            }
        }
        missing.sort(Comparator.comparingInt((SectionKey k) -> -k.level())
                .thenComparingDouble(k -> selection.distanceTo(k, lastEyeX, lastEyeZ)));
        for (SectionKey key : missing) {
            if (inFlight.get() >= maxInFlight) {
                break;
            }
            if (pending.add(key)) {
                inFlight.incrementAndGet();
                GameScheduler.scheduleParallel("Distant terrain", () -> build(key));
            }
        }
    }

    private void build(SectionKey key) {
        try {
            if (disposed) {
                return;
            }
            long start = System.nanoTime();
            ColumnSection columns = summarizer.summarize(key);
            int[] origin = key.netOrigin(geometry);
            ChunkMesh mesh = mesher.mesh(columns, origin, this::runs);
            int[] range = heightRange(columns);
            Node node = new Node(key, columns, origin, mesh, range[0], range[1]);
            node.lastWanted = System.nanoTime();
            if (!disposed) {
                Node old = nodes.put(key, node);
                if (old != null) {
                    node.previous = old.isDrawable() ? old : old.previous;
                }
                built.add(node);
            }
            buildNanos.addAndGet(System.nanoTime() - start);
            buildCount.incrementAndGet();
        } catch (RuntimeException e) {
            logger.error("Could not build distant node {}", key, e);
        } finally {
            pending.remove(key);
            inFlight.decrementAndGet();
        }
    }

    private void evict(long now) {
        Set<SectionKey> keep = new HashSet<>();
        for (Node node : shown) {
            keep.add(node.key);
        }
        for (Node node : new ArrayList<>(nodes.values())) {
            if (!node.uploaded || node.previous != null || keep.contains(node.key)
                    || now - node.lastWanted < KEEP_NANOS) {
                continue;
            }
            // Top levels are cheap and stand in for everything else: they stay.
            if (node.key.level() >= SectionKey.TOP_LEVEL - 1) {
                continue;
            }
            nodes.remove(node.key);
            node.mesh.dispose();
            node.mesh = null;
        }
    }

    /**
     * The runs of a column of the net at a level, read from whichever built node covers it
     * after folding — the same level if it exists, else the nearest coarser one.
     */
    int[] runs(int face, int netX, int netZ, int level) {
        FacePos pos = geometry.fold(face, netX + 0.5, netZ + 0.5);
        if (pos.face() == CubePattern.NO_FACE) {
            return null;
        }
        int u = (int) Math.floor(pos.u());
        int v = (int) Math.floor(pos.v());
        int faceBlocks = geometry.faceBlocks();
        if (u < 0 || v < 0 || u >= faceBlocks || v >= faceBlocks) {
            return null;
        }
        for (int l = level; l <= SectionKey.TOP_LEVEL; l++) {
            int span = SectionKey.columns(geometry) << l;
            Node node = nodes.get(new SectionKey(pos.face(), l, u / span, v / span));
            if (node != null) {
                int stride = 1 << l;
                return mesher.runs(node.columns, (u % span) / stride, (v % span) / stride);
            }
        }
        return null;
    }

    private static SectionKey parentOf(SectionKey key) {
        if (key.level() >= SectionKey.TOP_LEVEL) {
            return null;
        }
        return new SectionKey(key.face(), key.level() + 1, key.i() / 2, key.j() / 2);
    }

    private static int[] heightRange(ColumnSection columns) {
        int low = Integer.MAX_VALUE;
        int high = Integer.MIN_VALUE;
        int size = columns.size();
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                int count = columns.count(u, v);
                if (count == 0) {
                    continue;
                }
                high = Math.max(high, columns.top(columns.segment(u, v, 0)));
                low = Math.min(low, columns.bottom(columns.segment(u, v, count - 1)));
            }
        }
        return low == Integer.MAX_VALUE ? new int[]{0, 0} : new int[]{low, high};
    }
}
