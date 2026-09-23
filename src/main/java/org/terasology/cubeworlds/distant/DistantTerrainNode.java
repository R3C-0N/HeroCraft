// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.rendering.CubeSphereProjection;
import org.terasology.cubeworlds.rendering.SphericalChunkVisibilityPolicy;
import org.terasology.engine.context.Context;
import org.terasology.engine.monitoring.PerformanceMonitor;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.dag.AbstractNode;
import org.terasology.engine.rendering.dag.dependencyConnections.BufferPairConnection;
import org.terasology.engine.rendering.dag.stateChanges.BindFbo;
import org.terasology.engine.rendering.dag.stateChanges.EnableFaceCulling;
import org.terasology.engine.rendering.dag.stateChanges.EnableMaterial;
import org.terasology.engine.rendering.dag.stateChanges.SetInputTexture2D;
import org.terasology.engine.rendering.dag.stateChanges.SetSphereProjection;
import org.terasology.engine.rendering.sphere.SphereProjection;
import org.terasology.engine.rendering.world.WorldRenderer;
import org.terasology.engine.world.WorldProvider;
import org.terasology.gestalt.assets.ResourceUrn;
import org.terasology.gestalt.naming.Name;
import org.terasology.joml.geom.AABBf;

import static org.lwjgl.opengl.GL11.GL_TEXTURE_2D;
import static org.lwjgl.opengl.GL11.glBindTexture;
import static org.lwjgl.opengl.GL13.GL_TEXTURE0;
import static org.lwjgl.opengl.GL13.glActiveTexture;
import static org.terasology.engine.rendering.primitives.ChunkMesh.RenderPhase.OPAQUE;

/**
 * The pass that draws the distant terrain, into the same buffers as the opaque blocks.
 * <p>
 * It is the opaque blocks pass with another list: the chunk material, the same projection, the same
 * atlas. What differs is told to the shader by one uniform, {@code distantTerrain}: tile the atlas
 * once per block of the world, and throw away every fragment standing on a column of chunks the game
 * already draws. The uniform is set back to zero at the end of the pass, since the material is
 * shared with every other chunk pass.
 * <p>
 * Lighting, haze and fog come after, from the buffers, and treat this geometry as they treat the
 * terrain: there is nothing distant about the light of the distance except that it was baked.
 */
public class DistantTerrainNode extends AbstractNode {

    private static final ResourceUrn CHUNK_MATERIAL_URN = new ResourceUrn("CoreRendering:chunk");
    private static final int SPHERE_TABLE_SLOT = 8;
    /** Past the atlas, its normals and heights, and the projection table. */
    private static final int HOLE_SLOT = 10;

    private final Context gameContext;
    private WorldRenderer worldRenderer;
    private WorldProvider worldProvider;
    private Camera activeCamera;
    private Material chunkMaterial;
    private int lastTriangles;
    private int lastDrawn;
    private int lastUnderHole;

    public DistantTerrainNode(String nodeUri, Name providingModule, Context context) {
        super(nodeUri, providingModule, context);
        gameContext = context;
        worldProvider = context.get(WorldProvider.class);
        addOutputBufferPairConnection(1);
    }

    @Override
    public void setDependencies(Context context) {
        worldRenderer = context.get(WorldRenderer.class);
        activeCamera = worldRenderer.getActiveCamera();

        addDesiredStateChange(new EnableFaceCulling());
        BufferPairConnection bufferPairConnection = getInputBufferPairConnection(1);
        addOutputBufferPairConnection(1, bufferPairConnection);
        addDesiredStateChange(new BindFbo(bufferPairConnection.getBufferPair().getPrimaryFbo()));
        addDesiredStateChange(new EnableMaterial(CHUNK_MATERIAL_URN));
        context.getMaybe(SphereProjection.class).ifPresent(projection ->
                addDesiredStateChange(new SetSphereProjection(SPHERE_TABLE_SLOT, projection,
                        CHUNK_MATERIAL_URN, activeCamera::getPosition)));
        chunkMaterial = getMaterial(CHUNK_MATERIAL_URN);
        addDesiredStateChange(new SetInputTexture2D(0, "engine:terrain", CHUNK_MATERIAL_URN, "textureAtlas"));
        addDesiredStateChange(new SetInputTexture2D(1, "engine:effects", CHUNK_MATERIAL_URN, "textureEffects"));
    }

    @Override
    public void process() {
        // Asked every frame: the render graph is built before the systems start, and the distant
        // terrain belongs to a system.
        DistantState state = gameContext.get(DistantState.class);
        DistantTerrain terrain = state == null ? null : state.terrain();
        DistantHole hole = state == null ? null : state.hole();
        SphereProjection sphere = gameContext.get(SphereProjection.class);
        if (terrain == null || hole == null || !(sphere instanceof CubeSphereProjection)) {
            return;
        }
        CubeSphereProjection projection = (CubeSphereProjection) sphere;
        state.setNode(this);
        PerformanceMonitor.startActivity("rendering/" + getUri());
        CubeGeometry geometry = projection.geometry();

        chunkMaterial.setFloat("time", worldProvider.getTime().getDays(), true);
        chunkMaterial.setFloat("clip", 0.0f, true);
        chunkMaterial.setMatrix4("projectionMatrix", activeCamera.getProjectionMatrix(), true);

        glActiveTexture(GL_TEXTURE0 + HOLE_SLOT);
        glBindTexture(GL_TEXTURE_2D, hole.textureId());
        glActiveTexture(GL_TEXTURE0);
        chunkMaterial.setInt("distantHole", HOLE_SLOT, true);
        chunkMaterial.setInt("distantTerrain", 1, true);

        Vector3fc cameraPosition = activeCamera.getPosition();
        Matrix4f model = new Matrix4f();
        Matrix4f modelView = new Matrix4f();
        Matrix3f normalMatrix = new Matrix3f();
        Vector3f centre = new Vector3f();
        Vector3f curved = new Vector3f();
        AABBf bounds = new AABBf();
        int triangles = 0;
        int drawn = 0;
        int skipped = 0;
        for (DistantTerrain.Node node : terrain.shown()) {
            if (!node.isDrawable()) {
                continue;
            }
            Vector3fc origin = node.renderPosition();
            int span = node.key().span(geometry);
            if (hole.covers((int) origin.x(), (int) origin.z(), span)) {
                skipped++;
                continue;
            }
            bounds.setMin(origin.x(), node.lowest(), origin.z())
                    .setMax(origin.x() + span, node.highest() + 1, origin.z() + span);
            // The horizon of the floor, exactly as for a chunk; then a sphere around the curved centre.
            if (!SphericalChunkVisibilityPolicy.withinHorizon(geometry, cameraPosition, bounds)) {
                continue;
            }
            bounds.center(centre);
            projection.curvedPosition(centre, curved);
            float radius = 0.5f * (float) Math.sqrt(2.0 * span * span
                    + (double) (node.highest() - node.lowest()) * (node.highest() - node.lowest())) + 1.0f;
            if (!activeCamera.getViewFrustum().testSphere(curved.x, curved.y, curved.z, radius)) {
                continue;
            }
            node.mesh().updateMaterial(chunkMaterial, origin, false);
            model.setTranslation(origin.x() - cameraPosition.x(), origin.y() - cameraPosition.y(),
                    origin.z() - cameraPosition.z());
            modelView.set(activeCamera.getViewMatrix()).mul(model);
            chunkMaterial.setMatrix4("modelViewMatrix", modelView, true);
            chunkMaterial.setMatrix3("normalMatrix", modelView.normal(normalMatrix), true);
            triangles += node.mesh().render(OPAQUE);
            drawn++;
        }
        chunkMaterial.setInt("distantTerrain", 0, true);
        lastTriangles = triangles;
        lastDrawn = drawn;
        lastUnderHole = skipped;
        worldRenderer.increaseTrianglesCount(triangles);
        PerformanceMonitor.endActivity();
    }

    /** Nodes and triangles drawn by the last frame, for the bench. */
    public String stats() {
        return "drawn=" + lastDrawn + " under_hole=" + lastUnderHole + " triangles=" + lastTriangles;
    }
}
