// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.rendering;

import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.world.ChunkVisibilityPolicy;
import org.terasology.engine.world.chunks.RenderableChunk;
import org.terasology.joml.geom.AABBfc;

/**
 * Visibility on a world whose vertices are bent by the shader.
 * <p>
 * The stock test asks whether a chunk's bounding box is in the frustum, which stops being the right
 * question the moment the geometry no longer sits inside that box: with a radius of two thousand
 * blocks the drop reaches a whole chunk at three hundred and fifty-five blocks out, and beyond that
 * chunks are culled while still plainly in view.
 * <p>
 * So the chunk is tested where it is actually drawn — a bounding sphere around its curved centre —
 * and then against the horizon of the floor of the world, the one thing the planet is guaranteed to
 * be made of. Within the loaded box that horizon culls nothing, wherever the eye stands:
 * {@code HorizonCullCensusTest} counts it. The first version took the horizon over sea level and
 * measured the separation as a straight line from the camera; it threw away 41 % of the loaded box
 * from a beach and everything below the surface chunk from underground, where the aimed ray hit
 * rock and the screen showed the sky. What the horizon is still for is what lies beyond the box.
 * <p>
 * The drop is the radius', not a setting: it reaches a chunk at {@code sqrt(2 * 32 * R)}, and it
 * will move if the face does.
 */
public class SphericalChunkVisibilityPolicy implements ChunkVisibilityPolicy {

    private final CubeSphereProjection projection;
    // Asked from the render thread, a hundred times a frame: nothing allocated per chunk.
    private final Vector3f centre = new Vector3f();
    private final Vector3f curved = new Vector3f();

    public SphericalChunkVisibilityPolicy(CubeSphereProjection projection) {
        this.projection = projection;
    }

    @Override
    public boolean isVisible(Camera camera, RenderableChunk chunk) {
        AABBfc bounds = chunk.getAABB();
        centreOf(bounds);
        float radius = boundingRadius(bounds);

        projection.curvedPosition(centre, curved);
        if (!camera.getViewFrustum().testSphere(curved.x, curved.y, curved.z, radius)) {
            return false;
        }
        return withinHorizon(camera, bounds);
    }

    @Override
    public boolean isVisibleReflected(Camera camera, RenderableChunk chunk) {
        AABBfc bounds = chunk.getAABB();
        centreOf(bounds);
        projection.curvedPosition(centre, curved);
        return camera.getViewFrustumReflected()
                .testSphere(curved.x, curved.y, curved.z, boundingRadius(bounds));
    }

    private void centreOf(AABBfc bounds) {
        centre.set((bounds.minX() + bounds.maxX()) * 0.5f,
                (bounds.minY() + bounds.maxY()) * 0.5f,
                (bounds.minZ() + bounds.maxZ()) * 0.5f);
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
     * The only thing that hides terrain on a planet is the planet, and the only part of it that is
     * always solid is the mantle. So the horizon is taken over the floor of the world
     * ({@link CubeGeometry#occluderRadius()}), never over the reference level: measured from sea
     * level, every chunk below it had no reach at all, and underground only what lay within one
     * chunk of the eye was drawn.
     * <p>
     * Everything is an angle at the centre of the planet, and the separation is the world's
     * ({@link CubeGeometry#angle}), not a straight line from the camera. A straight line counts the
     * drop into the ground as distance, which is what used to cull the chunk under one's feet.
     */
    private boolean withinHorizon(Camera camera, AABBfc bounds) {
        return withinHorizon(projection.geometry(), camera.getPosition(), bounds);
    }

    /**
     * The horizon test on its own, with an eye instead of a camera, so that a census can put the eye
     * anywhere without a renderer.
     */
    public static boolean withinHorizon(CubeGeometry geometry, Vector3fc eye, AABBfc bounds) {
        double centreX = (bounds.minX() + bounds.maxX()) * 0.5;
        double centreZ = (bounds.minZ() + bounds.maxZ()) * 0.5;
        double separation = geometry.angle(eye.x(), eye.z(), centreX, centreZ);
        if (Double.isInfinite(separation)) {
            // Either the eye or the chunk is off the net. Nothing to decide on, so nothing is culled
            // here: the frustum has already had its say.
            return true;
        }
        double eyeRadius = geometry.renderRadius(eye.x(), eye.y(), eye.z());
        double topRadius = geometry.renderRadius(centreX, bounds.maxY(), centreZ);
        // The chunk is not a point: its far corner can stand clear while its centre is hidden. Half
        // its horizontal diagonal, at the largest a block gets, over the smallest radius involved.
        double dx = bounds.maxX() - bounds.minX();
        double dz = bounds.maxZ() - bounds.minZ();
        double halfWidth = 0.5 * Math.sqrt(dx * dx + dz * dz) * CubeGeometry.MAX_SCALE_FACTOR;
        double slack = halfWidth / geometry.occluderRadius();
        return separation <= geometry.horizonAngle(eyeRadius) + geometry.horizonAngle(topRadius) + slack;
    }
}
