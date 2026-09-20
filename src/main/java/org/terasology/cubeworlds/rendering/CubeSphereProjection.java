// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.rendering;

import org.joml.Matrix3f;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.lwjgl.BufferUtils;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL12;
import org.lwjgl.opengl.GL30;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.cubeworlds.topology.FacePos;
import org.terasology.engine.context.Context;
import org.terasology.engine.rendering.sphere.SphereProjection;
import org.terasology.engine.rendering.world.WorldRenderer;

import java.nio.FloatBuffer;

/**
 * The cube net, seen by the renderer.
 * <p>
 * Holds the one copy of the projection table that both sides read: the CPU reads the float array
 * for culling and picking, the GPU reads the very same values out of a texture. Nothing is
 * quantised on the way, so there is no version of the truth that only one of them believes.
 */
public class CubeSphereProjection implements SphereProjection {

    /**
     * Three channels of full float. The asset path cannot carry them — {@code TextureData} fixes
     * four bytes of RGBA per pixel — so the texture is allocated directly. Packed into bytes
     * instead, sixteen bits per channel would misplace a vertex by a quarter of a block at the
     * centre of a face, in a checkerboard the size of a texel.
     */
    private static final int INTERNAL_FORMAT = GL30.GL_RGB32F;

    private final CubeGeometry geometry;
    private final Context context;
    private final Vector3f defaultFocus = new Vector3f();
    private final Vector3f focus = new Vector3f();
    private final double[] direction = new double[3];

    private int textureId;
    private WorldRenderer worldRenderer;

    public CubeSphereProjection(CubeGeometry geometry, Context context) {
        this.geometry = geometry;
        this.context = context;
        int[] centre = geometry.faceCentre(1);
        defaultFocus.set(centre[0], geometry.seaLevel(), centre[1]);
    }

    public CubeGeometry geometry() {
        return geometry;
    }

    @Override
    public float getRadius() {
        return (float) geometry.radius();
    }

    @Override
    public float getReferenceHeight() {
        return geometry.seaLevel();
    }

    @Override
    public Vector3fc getFocus() {
        if (worldRenderer == null && context != null) {
            worldRenderer = context.get(WorldRenderer.class);
        }
        if (worldRenderer == null || worldRenderer.getActiveCamera() == null) {
            return defaultFocus;
        }
        return focus.set(worldRenderer.getActiveCamera().getPosition());
    }

    @Override
    public int getTableResolution() {
        return geometry.table().resolution();
    }

    @Override
    public float getFaceEdge() {
        return geometry.faceBlocks();
    }

    /**
     * Uploads on first use. Called from a state change, so the GL context is current; doing it in a
     * constructor would run on whichever thread built the world.
     */
    @Override
    public int getTableTextureId() {
        if (textureId != 0) {
            return textureId;
        }
        ConformalCubeTable table = geometry.table();
        int n = table.resolution();
        float[] data = table.data();
        FloatBuffer buffer = BufferUtils.createFloatBuffer(data.length);
        buffer.put(data);
        buffer.flip();

        textureId = GL11.glGenTextures();
        GL11.glBindTexture(GL11.GL_TEXTURE_2D, textureId);
        GL11.glTexImage2D(GL11.GL_TEXTURE_2D, 0, INTERNAL_FORMAT, n, n, 0,
                GL11.GL_RGB, GL11.GL_FLOAT, buffer);
        // Read with texelFetch and interpolated by hand, exactly as the CPU does. Hardware
        // filtering would be a second opinion, and the two would not agree.
        GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL11.GL_TEXTURE_MIN_FILTER, GL11.GL_NEAREST);
        GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL11.GL_TEXTURE_MAG_FILTER, GL11.GL_NEAREST);
        GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL11.GL_TEXTURE_WRAP_S, GL12.GL_CLAMP_TO_EDGE);
        GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL11.GL_TEXTURE_WRAP_T, GL12.GL_CLAMP_TO_EDGE);
        GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL12.GL_TEXTURE_MAX_LEVEL, 0);
        return textureId;
    }

    @Override
    public Vector3f curvedPosition(Vector3fc worldPosition, Vector3f dest) {
        Vector3fc origin = getFocus();
        curved(worldPosition.x(), worldPosition.y(), worldPosition.z(), dest);
        Vector3f originCurved = new Vector3f();
        curved(origin.x(), origin.y(), origin.z(), originCurved);
        dest.sub(originCurved);
        // Back into world axes, exactly as the shader does. Planet space is not world space, and
        // whoever receives this — chunk culling, entity placement — works in world axes. The two
        // must agree to the bit or a chunk is culled on evidence from somewhere it is not.
        return frameAtAbs(origin, new Matrix3f()).transpose().transform(dest);
    }

    /** Absolute curved position, in planet space, before the focus is taken off. */
    private void curved(double x, double y, double z, Vector3f dest) {
        int face = geometry.faceOf((int) Math.floor(x), (int) Math.floor(z));
        if (face == CubePattern.NO_FACE) {
            dest.set(0.0f, 0.0f, (float) geometry.radius());
            return;
        }
        FacePos pos = geometry.fold(face, x, z);
        geometry.direction(pos, direction);
        double scale = geometry.scaleFactor(pos);
        double r = geometry.radius() + scale * (y - geometry.seaLevel());
        dest.set((float) (direction[0] * r), (float) (direction[1] * r), (float) (direction[2] * r));
    }

    @Override
    public Matrix3f frameAt(Vector3fc worldPosition, Matrix3f dest) {
        // Relative to the focus, like the shader: the identity under the camera, turning by d over R
        // away from it. The absolute frame would hand a planet-space vector to a world-space caller.
        Matrix3f atFocus = frameAtAbs(getFocus(), new Matrix3f()).transpose();
        return atFocus.mul(frameAtAbs(worldPosition, dest), dest);
    }

    private Matrix3f frameAtAbs(Vector3fc worldPosition, Matrix3f dest) {
        // Finite differences on the same table the shader uses, so the two frames agree. The step
        // is a fraction of a face rather than a block: closer than that and single precision noise
        // in the table starts to dominate the difference.
        float step = geometry.faceBlocks() / 256.0f;
        Vector3f up = unitDirection(worldPosition.x(), worldPosition.z(), new Vector3f());
        Vector3f alongX = unitDirection(worldPosition.x() + step, worldPosition.z(), new Vector3f())
                .sub(unitDirection(worldPosition.x() - step, worldPosition.z(), new Vector3f()));
        Vector3f alongZ = unitDirection(worldPosition.x(), worldPosition.z() + step, new Vector3f())
                .sub(unitDirection(worldPosition.x(), worldPosition.z() - step, new Vector3f()));

        Vector3f east = new Vector3f(alongX).sub(new Vector3f(up).mul(alongX.dot(up)));
        Vector3f north = new Vector3f(alongZ).sub(new Vector3f(up).mul(alongZ.dot(up)));
        if (east.lengthSquared() < 1e-12f || north.lengthSquared() < 1e-12f) {
            return dest.identity();
        }
        east.normalize();
        north.normalize();
        // world X, Y, Z map onto east, up, north
        return dest.set(east, up, north);
    }

    private Vector3f unitDirection(double x, double z, Vector3f dest) {
        int face = geometry.faceOf((int) Math.floor(x), (int) Math.floor(z));
        if (face == CubePattern.NO_FACE) {
            return dest.set(0.0f, 0.0f, 1.0f);
        }
        double[] local = new double[3];
        FacePos pos = geometry.fold(face, x, z);
        geometry.direction(pos, local);
        return dest.set((float) local[0], (float) local[1], (float) local[2]);
    }

    @Override
    public float horizon(float heightAboveReference) {
        return (float) geometry.horizon(heightAboveReference);
    }
}
