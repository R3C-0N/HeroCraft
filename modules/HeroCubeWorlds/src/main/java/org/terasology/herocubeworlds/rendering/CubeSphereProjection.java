// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.rendering;

import org.joml.Matrix3f;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.lwjgl.BufferUtils;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL12;
import org.lwjgl.opengl.GL30;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.projection.ConformalCubeTable;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FaceBasis;
import org.terasology.herocubeworlds.topology.FacePos;
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
    private final double[] owned = new double[2];

    /*
     * What depends on the focus alone, kept until it moves. The focus is the camera, and every chunk
     * and every entity asks for its curved position a frame: each of those calls used to bend the
     * focus again and rebuild its frame from five table reads, with a dozen allocations on the way.
     */
    private final Vector3f cachedFocus = new Vector3f(Float.NaN, Float.NaN, Float.NaN);
    private final Vector3f focusCurved = new Vector3f();
    private final Matrix3f focusToWorld = new Matrix3f();
    private final Vector3f shaderFocusPoint = new Vector3f();
    private final Matrix3f shaderFocusFrame = new Matrix3f();

    // Scratch for the frames, which are asked from the render thread only.
    private final Vector3f up = new Vector3f();
    private final Vector3f alongX = new Vector3f();
    private final Vector3f alongZ = new Vector3f();
    private final Vector3f east = new Vector3f();
    private final Vector3f north = new Vector3f();
    private final Vector3f scratch = new Vector3f();
    private final Matrix3f frameScratch = new Matrix3f();
    private final float[] shaderUp = new float[4];
    private final float[] shaderPlus = new float[4];
    private final float[] shaderMinus = new float[4];
    private final float[] shaderSample = new float[ConformalCubeTable.CHANNELS];

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
    public boolean isOutside(int blockX, int blockZ) {
        return geometry.faceOf(blockX, blockZ) == CubePattern.NO_FACE;
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

    /**
     * Where a position that must not be seen is sent, as the shader's {@code SPHERE_EXILE}: far below
     * the focus, past any far plane, so a frustum test turns it down.
     */
    public static final float EXILE = 1.0e7f;

    @Override
    public Vector3f focusPoint(Vector3f dest) {
        refreshFocus();
        return dest.set(shaderFocusPoint);
    }

    @Override
    public Matrix3f focusFrame(Matrix3f dest) {
        refreshFocus();
        return dest.set(shaderFocusFrame);
    }

    @Override
    public Vector3f curvedPosition(Vector3fc worldPosition, Vector3f dest) {
        Vector3fc origin = getFocus();
        // The pockets of the past live in the dead slots of the net, and they are flat. Seen from
        // inside one they are drawn as stored; across the line, either way, nothing is seen. Dead
        // means out of reach of every face: the corner of a block on a cut seam lies half a block
        // into the slot beside it, and belongs to its face, not to a pocket.
        boolean hereDead = ownFace(worldPosition.x(), worldPosition.z(), owned) == CubePattern.NO_FACE;
        boolean focusDead = ownFace(origin.x(), origin.z(), owned) == CubePattern.NO_FACE;
        if (hereDead || focusDead) {
            return hereDead && focusDead ? dest.set(worldPosition).sub(origin) : dest.set(0.0f, -EXILE, 0.0f);
        }
        refreshFocus();
        curved(worldPosition.x(), worldPosition.y(), worldPosition.z(), dest);
        dest.sub(focusCurved);
        // Back into world axes, exactly as the shader does. Planet space is not world space, and
        // whoever receives this — chunk culling, entity placement — works in world axes. The two
        // must agree to the bit or a chunk is culled on evidence from somewhere it is not.
        return focusToWorld.transform(dest);
    }

    /** Where the focus is while no camera exists: a test puts it anywhere. */
    void setDefaultFocus(float x, float y, float z) {
        defaultFocus.set(x, y, z);
    }

    /** The focus bent in double precision, the CPU's own path: for the test that holds the two together. */
    Vector3f cpuFocusPoint(Vector3f dest) {
        refreshFocus();
        return dest.set(focusCurved);
    }

    /** The CPU's own frame at the focus, likewise. */
    Matrix3f cpuFocusFrame(Matrix3f dest) {
        refreshFocus();
        return dest.set(focusToWorld);
    }

    /** Bends the focus and takes its frame again, if and only if it has moved since the last time. */
    private void refreshFocus() {
        Vector3fc origin = getFocus();
        if (origin.equals(cachedFocus)) {
            return;
        }
        cachedFocus.set(origin);
        curved(origin.x(), origin.y(), origin.z(), focusCurved);
        frameAtAbs(origin, focusToWorld).transpose();
        shaderFocus(origin);
    }

    /**
     * The shader's SPHERE_NUDGES: where to look for the face a point of a dead slot is half a block from. A block is
     * centred on its coordinates, so the outer corners of the blocks on a cut seam lie half a block into the dead slot
     * beside it, and are brought back onto the edge of their own face.
     */
    private static final double[][] NUDGES = {
            {0.5, 0}, {-0.5, 0}, {0, 0.5}, {0, -0.5}, {0.5, 0.5}, {-0.5, 0.5}, {0.5, -0.5}, {-0.5, -0.5}};

    /** The face of a point, once brought out of a dead slot; its coordinates are moved into {@code at}. */
    private int ownFace(double x, double z, double[] at) {
        at[0] = x;
        at[1] = z;
        int face = geometry.faceOf((int) Math.floor(x), (int) Math.floor(z));
        for (int i = 0; i < NUDGES.length && face == CubePattern.NO_FACE; i++) {
            at[0] = x + NUDGES[i][0];
            at[1] = z + NUDGES[i][1];
            face = geometry.faceOf((int) Math.floor(at[0]), (int) Math.floor(at[1]));
        }
        return face;
    }

    /** Absolute curved position, in planet space, before the focus is taken off. */
    private void curved(double x, double y, double z, Vector3f dest) {
        int face = ownFace(x, z, owned);
        if (face == CubePattern.NO_FACE) {
            dest.set(0.0f, 0.0f, (float) geometry.radius());
            return;
        }
        FacePos pos = geometry.fold(face, owned[0], owned[1]);
        geometry.direction(pos, direction);
        double scale = geometry.scaleFactor(pos);
        double r = geometry.radius() + scale * (y - geometry.seaLevel());
        dest.set((float) (direction[0] * r), (float) (direction[1] * r), (float) (direction[2] * r));
    }

    @Override
    public Matrix3f frameAt(Vector3fc worldPosition, Matrix3f dest) {
        // Relative to the focus, like the shader: the identity under the camera, turning by d over R
        // away from it. The absolute frame would hand a planet-space vector to a world-space caller.
        refreshFocus();
        frameAtAbs(worldPosition, frameScratch);
        return focusToWorld.mul(frameScratch, dest);
    }

    private Matrix3f frameAtAbs(Vector3fc worldPosition, Matrix3f dest) {
        // Finite differences on the same table the shader uses, so the two frames agree. The step
        // is a fraction of a face rather than a block: closer than that and single precision noise
        // in the table starts to dominate the difference.
        float step = geometry.faceBlocks() / 256.0f;
        unitDirection(worldPosition.x(), worldPosition.z(), up);
        unitDirection(worldPosition.x() + step, worldPosition.z(), alongX)
                .sub(unitDirection(worldPosition.x() - step, worldPosition.z(), scratch));
        unitDirection(worldPosition.x(), worldPosition.z() + step, alongZ)
                .sub(unitDirection(worldPosition.x(), worldPosition.z() - step, scratch));

        // along - up * dot(along, up), in that order, as before
        east.set(up).mul(alongX.dot(up)).negate().add(alongX);
        north.set(up).mul(alongZ.dot(up)).negate().add(alongZ);
        if (east.lengthSquared() < 1e-12f || north.lengthSquared() < 1e-12f) {
            return dest.identity();
        }
        east.normalize();
        north.normalize();
        // world X, Y, Z map onto east, up, north
        return dest.set(east, up, north);
    }

    private Vector3f unitDirection(double x, double z, Vector3f dest) {
        int face = ownFace(x, z, owned);
        if (face == CubePattern.NO_FACE) {
            return dest.set(0.0f, 0.0f, 1.0f);
        }
        FacePos pos = geometry.fold(face, owned[0], owned[1]);
        geometry.direction(pos, direction);
        return dest.set((float) direction[0], (float) direction[1], (float) direction[2]);
    }

    // -- The focus as the shader sees it ------------------------------------------------------
    //
    // A line by line transcription of sphereDirection and sphereFrameAbs, in single precision and in
    // the shader's order. The vertex subtracts this point from its own bent position; computed in
    // double instead, the two would no longer cancel under the camera, and the ground at the feet
    // would sit a few thousandths of a block off, moving as the camera moves.

    private void shaderFocus(Vector3fc origin) {
        shaderDirection(origin.x(), origin.z(), shaderUp);
        float r0 = (float) geometry.radius() + shaderUp[3] * (origin.y() - geometry.seaLevel());
        shaderFocusPoint.set(shaderUp[0] * r0, shaderUp[1] * r0, shaderUp[2] * r0);

        float step = geometry.faceBlocks() / 256.0f;
        shaderDirection(origin.x() + step, origin.z(), shaderPlus);
        shaderDirection(origin.x() - step, origin.z(), shaderMinus);
        Vector3f e = tangent(shaderPlus, shaderMinus, shaderUp, east);
        shaderDirection(origin.x(), origin.z() + step, shaderPlus);
        shaderDirection(origin.x(), origin.z() - step, shaderMinus);
        Vector3f n = tangent(shaderPlus, shaderMinus, shaderUp, north);
        // transpose(mat3(east, up, north)): the columns are the three axes, then turned over.
        shaderFocusFrame.set(e.x, e.y, e.z, shaderUp[0], shaderUp[1], shaderUp[2], n.x, n.y, n.z).transpose();
    }

    /** normalize(along - up * dot(along, up)), with along the central difference. */
    private static Vector3f tangent(float[] plus, float[] minus, float[] up, Vector3f dest) {
        float ax = plus[0] - minus[0];
        float ay = plus[1] - minus[1];
        float az = plus[2] - minus[2];
        float d = ax * up[0] + ay * up[1] + az * up[2];
        return dest.set(ax - up[0] * d, ay - up[1] * d, az - up[2] * d).normalize();
    }

    /** sphereDirection: the unit direction of a column in planet space, and its scale factor. */
    private void shaderDirection(float x, float z, float[] out) {
        shaderDirectionAt(x, z, out);
        for (int i = 0; i < NUDGES.length && out[3] < 0; i++) {
            shaderDirectionAt(x + (float) NUDGES[i][0], z + (float) NUDGES[i][1], out);
        }
        if (out[3] < 0) {
            out[0] = 0.0f;
            out[1] = 0.0f;
            out[2] = 1.0f;
            out[3] = 1.0f;
        }
    }

    /** One try of sphereFaceOf and the table; a negative scale factor says the point is in a dead slot. */
    private void shaderDirectionAt(float x, float z, float[] out) {
        float edge = geometry.faceBlocks();
        float slotX = (float) Math.floor(x / edge);
        float slotZ = (float) Math.floor(z / edge);
        float localX = x - slotX * edge;
        float localZ = z - slotZ * edge;
        int face = CubePattern.NO_FACE;
        for (int f = 0; f < CubePattern.FACES; f++) {
            if (CubePattern.PATTERN[f][0] == slotX && CubePattern.PATTERN[f][1] == slotZ) {
                face = f;
                break;
            }
        }
        if (face == CubePattern.NO_FACE) {
            out[3] = -1.0f;
            return;
        }
        shaderTableAt(2.0f * localX / edge - 1.0f, 2.0f * localZ / edge - 1.0f, shaderSample);
        FaceBasis b = FaceBasis.BASES[face];
        float dx = b.nx() + b.rx() * shaderSample[0] + b.hx() * shaderSample[1];
        float dy = b.ny() + b.ry() * shaderSample[0] + b.hy() * shaderSample[1];
        float dz = b.nz() + b.rz() * shaderSample[0] + b.hz() * shaderSample[1];
        float inverse = (float) (1.0 / Math.sqrt(dx * dx + dy * dy + dz * dz));
        out[0] = dx * inverse;
        out[1] = dy * inverse;
        out[2] = dz * inverse;
        out[3] = shaderSample[2];
    }

    /** sphereTableAt: the four texels and the hand-written bilinear weights, clamp included. */
    private void shaderTableAt(float s, float t, float[] out) {
        ConformalCubeTable table = geometry.table();
        float[] data = table.data();
        int size = table.resolution();
        float n = size - 1;
        float x = Math.max(0.0f, Math.min(n - 0.0001f, (s + 1.0f) * 0.5f * n));
        float y = Math.max(0.0f, Math.min(n - 0.0001f, (t + 1.0f) * 0.5f * n));
        int ix = (int) x;
        int iy = (int) y;
        float fx = x - ix;
        float fy = y - iy;
        int channels = ConformalCubeTable.CHANNELS;
        for (int c = 0; c < channels; c++) {
            float a = data[channels * (iy * size + ix) + c];
            float b = data[channels * (iy * size + ix + 1) + c];
            float cc = data[channels * ((iy + 1) * size + ix) + c];
            float d = data[channels * ((iy + 1) * size + ix + 1) + c];
            float low = a + (b - a) * fx;
            float high = cc + (d - cc) * fx;
            out[c] = low + (high - low) * fy;
        }
    }

    @Override
    public float horizon(float heightAboveReference) {
        return (float) geometry.horizon(heightAboveReference);
    }
}
