// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.rendering.CubeSphereProjection;
import org.terasology.herocubeworlds.rendering.SphericalChunkVisibilityPolicy;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.joml.geom.AABBf;

import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;

import static org.lwjgl.opengl.GL11.GL_TEXTURE_2D;
import static org.lwjgl.opengl.GL11.glBindTexture;
import static org.lwjgl.opengl.GL13.GL_TEXTURE0;
import static org.lwjgl.opengl.GL13.glActiveTexture;

/**
 * Draws the distant terrain from inside the passes that draw the chunks.
 * <p>
 * The opaque blocks pass, the water pass and the shadow map each call this once they have drawn
 * their chunks, with their material still bound and their state still set — the buffer, the atlas,
 * the normals and heights when they are on, the water's reflection and refraction, the light's
 * camera. So the distance is drawn exactly as the terrain is, by construction, and there is no
 * second copy of any pass to keep in step with the first. What differs is told to the shader by one
 * uniform, {@code distantTerrain}: tile the atlas once per block of the world, and throw away every
 * fragment standing on a column of chunks the game already draws. It is set back to zero before
 * returning, since the material is the chunks' own.
 * <p>
 * A node is culled as a chunk is: the horizon of the floor, then a sphere round its curved centre
 * against the frustum. The light's camera is not an eye and has no horizon: for it a node is kept if
 * it comes within reach of the map, measured along the surface. The map is redrawn on
 * its own schedule — when its chunks change, and every few seconds — so a distant node that has just
 * arrived casts its shadow at the next of those.
 */
public final class DistantDrawer {

    /** Past the atlas, its normals and heights, the water's textures and the projection table. */
    private static final int HOLE_SLOT = 10;
    /**
     * How far from the shadow camera, along the surface, a node can reach into the shadow map: the map
     * is four hundred blocks square round the eye, whose corner is at √2 times two hundred, and the
     * camera stands up to sixty-four blocks off the eye along the light.
     */
    private static final double SHADOW_REACH = 200.0 * Math.sqrt(2.0) + 64.0;
    /**
     * The coarsest level that casts a shadow. The map reaches two hundred blocks from the eye, where
     * the first two levels are; a coarser node there only stands in while they load, and drawing its
     * whole span into the map for the corner of it the map covers costs more than the shadow is worth.
     */
    private static final int SHADOW_LEVEL = 2;

    private final DistantTerrain terrain;
    private final DistantHole hole;
    private final CubeSphereProjection projection;
    private final Map<String, String> lastStats = new TreeMap<>();

    public DistantDrawer(DistantTerrain terrain, DistantHole hole, CubeSphereProjection projection) {
        this.terrain = terrain;
        this.hole = hole;
        this.projection = projection;
    }

    /** @see org.terasology.engine.rendering.world.DistantGeometry#render */
    public int render(ChunkMesh.RenderPhase phase, Material material, Camera camera, boolean shadow) {
        CubeGeometry geometry = projection.geometry();
        Vector3fc focus = projection.getFocus();
        if (geometry.faceOf((int) Math.floor(focus.x()), (int) Math.floor(focus.z())) == CubePattern.NO_FACE) {
            // The eye is in a pocket of the past, on a dead slot of the net: the planet is not there
            // to be seen, and the shader would exile every node anyway, at the cost of drawing it.
            return 0;
        }
        glActiveTexture(GL_TEXTURE0 + HOLE_SLOT);
        glBindTexture(GL_TEXTURE_2D, hole.textureId());
        glActiveTexture(GL_TEXTURE0);
        material.setInt("distantHole", HOLE_SLOT, true);
        material.setInt("distantTerrain", 1, true);

        Vector3fc cameraPosition = camera.getPosition();
        Vector3fc eye = shadow ? null : cameraPosition;
        Matrix4f model = new Matrix4f();
        Matrix4f modelView = new Matrix4f();
        Matrix3f normalMatrix = new Matrix3f();
        Vector3f centre = new Vector3f();
        Vector3f curved = new Vector3f();
        AABBf bounds = new AABBf();
        int triangles = 0;
        int drawn = 0;
        int underHole = 0;
        for (DistantTerrain.Node node : terrain.shown()) {
            if (!node.isDrawable() || shadow && node.key().level() > SHADOW_LEVEL) {
                continue;
            }
            Vector3fc origin = node.renderPosition();
            int span = node.key().span(geometry);
            // The light's frustum is a box a few hundred blocks wide, and it is tested along the surface,
            // as the nodes are chosen: a sphere round a curved centre is no test of it.
            if (shadow && nearest(geometry, node, span, cameraPosition) > SHADOW_REACH) {
                continue;
            }
            if (hole.covers(node.gridX(), node.gridZ(), span)) {
                underHole++;
                continue;
            }
            bounds.setMin(origin.x(), node.lowest() - 0.5f, origin.z())
                    .setMax(origin.x() + span, node.highest() + 0.5f, origin.z() + span);
            // The horizon of the floor, exactly as for a chunk; then a sphere around the curved centre.
            if (eye != null && !SphericalChunkVisibilityPolicy.withinHorizon(geometry, eye, bounds)) {
                continue;
            }
            bounds.center(centre);
            projection.curvedPosition(centre, curved);
            float radius = 0.5f * (float) Math.sqrt(2.0 * span * span
                    + (double) (node.highest() - node.lowest()) * (node.highest() - node.lowest())) + 1.0f;
            if (!shadow && !camera.getViewFrustum().testSphere(curved.x, curved.y, curved.z, radius)) {
                continue;
            }
            node.mesh().updateMaterial(material, origin, false);
            model.setTranslation(origin.x() - cameraPosition.x(), origin.y() - cameraPosition.y(),
                    origin.z() - cameraPosition.z());
            modelView.set(camera.getViewMatrix()).mul(model);
            material.setMatrix4("modelViewMatrix", modelView, true);
            material.setMatrix3("normalMatrix", modelView.normal(normalMatrix), true);
            node.mesh().render(phase);
            // What this phase drew: render answers for every phase of the mesh at once.
            triangles += node.mesh().triangleCount(phase);
            drawn++;
        }
        material.setInt("distantTerrain", 0, true);
        String pass = shadow ? "shadow" : phase.name().toLowerCase(Locale.ROOT);
        lastStats.put(pass, pass + "=" + drawn + "/" + triangles + (shadow ? "" : " under_hole=" + underHole));
        return triangles;
    }

    /**
     * Before a pass draws its chunks: the hole bound, so that those along the edge give way to the
     * distance by the dither; after it has drawn the distance, off again.
     */
    public void fadeChunks(Material material, boolean on) {
        if (on) {
            glActiveTexture(GL_TEXTURE0 + HOLE_SLOT);
            glBindTexture(GL_TEXTURE_2D, hole.textureId());
            glActiveTexture(GL_TEXTURE0);
            material.setInt("distantHole", HOLE_SLOT, true);
        }
        material.setInt("distantFade", on ? 1 : 0, true);
    }

    /** How near a node comes to a point, along the surface: its centre's distance less its half diagonal. */
    private static double nearest(CubeGeometry geometry, DistantTerrain.Node node, int span, Vector3fc point) {
        double half = span / 2.0;
        double arc = geometry.arc(point.x(), point.z(), node.gridX() + half, node.gridZ() + half);
        return arc - half * Math.sqrt(2.0) * CubeGeometry.MAX_SCALE_FACTOR;
    }

    /** Nodes and triangles drawn by each pass at its last frame, as pass=nodes/triangles, for the bench. */
    public String stats() {
        return lastStats.isEmpty() ? "pass=absent" : String.join(" ", lastStats.values());
    }
}
