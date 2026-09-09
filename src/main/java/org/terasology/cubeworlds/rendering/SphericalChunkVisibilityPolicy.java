// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.rendering;

import org.joml.Vector3f;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.world.ChunkVisibilityPolicy;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.chunks.RenderableChunk;
import org.terasology.joml.geom.AABBfc;

/**
 * Visibility on a world whose vertices are bent by the shader.
 * <p>
 * The stock test asks whether a chunk's bounding box is in the frustum, which stops being the right
 * question the moment the geometry no longer sits inside that box: with a radius of five thousand
 * blocks the drop reaches a whole chunk at five hundred and sixty-eight blocks out, and beyond that
 * chunks are culled while still plainly in view.
 * <p>
 * So the chunk is tested where it is actually drawn — a bounding sphere around its curved centre —
 * and then against the horizon, which on a curved world is a real limit and a much tighter one than
 * the view distance: a hundred and forty-two blocks at eye height. Most of the loaded world is
 * behind it, so this is as much an optimisation as a correction.
 */
public class SphericalChunkVisibilityPolicy implements ChunkVisibilityPolicy {

    private final CubeSphereProjection projection;

    public SphericalChunkVisibilityPolicy(CubeSphereProjection projection) {
        this.projection = projection;
    }

    @Override
    public boolean isVisible(Camera camera, RenderableChunk chunk) {
        AABBfc bounds = chunk.getAABB();
        Vector3f centre = new Vector3f(
                (bounds.minX() + bounds.maxX()) * 0.5f,
                (bounds.minY() + bounds.maxY()) * 0.5f,
                (bounds.minZ() + bounds.maxZ()) * 0.5f);
        float radius = boundingRadius(bounds);

        Vector3f curved = projection.curvedPosition(centre, new Vector3f());
        if (!camera.getViewFrustum().testSphere(curved.x, curved.y, curved.z, radius)) {
            return false;
        }
        return withinHorizon(camera, centre, bounds);
    }

    @Override
    public boolean isVisibleReflected(Camera camera, RenderableChunk chunk) {
        AABBfc bounds = chunk.getAABB();
        Vector3f centre = new Vector3f(
                (bounds.minX() + bounds.maxX()) * 0.5f,
                (bounds.minY() + bounds.maxY()) * 0.5f,
                (bounds.minZ() + bounds.maxZ()) * 0.5f);
        Vector3f curved = projection.curvedPosition(centre, new Vector3f());
        return camera.getViewFrustumReflected()
                .testSphere(curved.x, curved.y, curved.z, boundingRadius(bounds));
    }

    /**
     * Half the diagonal, scaled up for the largest a block can get and with a block of slack for
     * the sag inside the chunk itself, which is a fortieth of a block at chunk size.
     */
    private float boundingRadius(AABBfc bounds) {
        float dx = bounds.maxX() - bounds.minX();
        float dy = bounds.maxY() - bounds.minY();
        float dz = bounds.maxZ() - bounds.minZ();
        return 0.5f * (float) Math.sqrt(dx * dx + dy * dy + dz * dz) + 1.0f;
    }

    /**
     * Whether the chunk pokes above the horizon seen from the eye.
     * <p>
     * Heights are taken above the reference level and never as raw Y: getting that reference wrong
     * makes the whole world disappear rather than fail visibly.
     */
    private boolean withinHorizon(Camera camera, Vector3f centre, AABBfc bounds) {
        float reference = projection.getReferenceHeight();
        float eyeHeight = camera.getPosition().y - reference;
        float chunkTop = bounds.maxY() - reference;

        double eyeReach = projection.horizon(Math.max(0.0f, eyeHeight));
        double chunkReach = projection.horizon(Math.max(0.0f, chunkTop));
        double separation = arcTo(centre, camera);
        return separation <= eyeReach + chunkReach + Chunks.SIZE_X;
    }

    /** Distance along the surface between a chunk centre and the camera, in blocks. */
    private double arcTo(Vector3f centre, Camera camera) {
        Vector3f relative = projection.curvedPosition(centre, new Vector3f());
        // The focus is the camera, so the chord to the origin is the chord between the two points.
        double chord = Math.sqrt(relative.x * relative.x + relative.y * relative.y + relative.z * relative.z);
        double radius = projection.getRadius();
        double half = Math.min(1.0, chord / (2.0 * radius));
        return 2.0 * radius * Math.asin(half);
    }
}
