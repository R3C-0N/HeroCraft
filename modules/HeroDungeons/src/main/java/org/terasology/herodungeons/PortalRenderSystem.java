// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.FrustumIntersection;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Vector2ic;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.joml.Vector4f;
import org.lwjgl.BufferUtils;
import org.terasology.engine.context.Context;
import org.terasology.engine.core.SimpleUri;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.RenderSystem;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.assets.mesh.StandardMeshData;
import org.terasology.engine.rendering.assets.shader.ShaderProgramFeature;
import org.terasology.engine.rendering.assets.texture.Texture;
import org.terasology.engine.rendering.backdrop.BackdropProvider;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.opengl.FBO;
import org.terasology.engine.rendering.primitives.ChunkMesh;
import org.terasology.engine.rendering.sphere.SphereProjection;
import org.terasology.engine.rendering.world.WorldRenderer;
import org.terasology.engine.utilities.Assets;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.generator.WorldGenerator;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import java.nio.IntBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;

import static org.lwjgl.opengl.GL11.GL_BACK;
import static org.lwjgl.opengl.GL11.GL_BLEND;
import static org.lwjgl.opengl.GL11.GL_COLOR_BUFFER_BIT;
import static org.lwjgl.opengl.GL11.GL_CULL_FACE;
import static org.lwjgl.opengl.GL11.GL_CULL_FACE_MODE;
import static org.lwjgl.opengl.GL11.GL_DEPTH_BUFFER_BIT;
import static org.lwjgl.opengl.GL11.GL_DEPTH_TEST;
import static org.lwjgl.opengl.GL11.GL_DEPTH_WRITEMASK;
import static org.lwjgl.opengl.GL11.GL_FLOAT;
import static org.lwjgl.opengl.GL11.GL_RGBA;
import static org.lwjgl.opengl.GL11.GL_TEXTURE_2D;
import static org.lwjgl.opengl.GL11.GL_TEXTURE_BINDING_2D;
import static org.lwjgl.opengl.GL11.GL_VIEWPORT;
import static org.lwjgl.opengl.GL11.glBindTexture;
import static org.lwjgl.opengl.GL11.glClear;
import static org.lwjgl.opengl.GL11.glClearColor;
import static org.lwjgl.opengl.GL11.glCullFace;
import static org.lwjgl.opengl.GL11.glDepthMask;
import static org.lwjgl.opengl.GL11.glDisable;
import static org.lwjgl.opengl.GL11.glEnable;
import static org.lwjgl.opengl.GL11.glGetBoolean;
import static org.lwjgl.opengl.GL11.glGetError;
import static org.lwjgl.opengl.GL11.glGetInteger;
import static org.lwjgl.opengl.GL11.glGetIntegerv;
import static org.lwjgl.opengl.GL11.glIsEnabled;
import static org.lwjgl.opengl.GL11.glReadPixels;
import static org.lwjgl.opengl.GL11.glViewport;
import static org.lwjgl.opengl.GL13.GL_ACTIVE_TEXTURE;
import static org.lwjgl.opengl.GL13.GL_TEXTURE0;
import static org.lwjgl.opengl.GL13.glActiveTexture;
import static org.lwjgl.opengl.GL20.GL_CURRENT_PROGRAM;
import static org.lwjgl.opengl.GL20.glUseProgram;
import static org.lwjgl.opengl.GL30.GL_FRAMEBUFFER;
import static org.lwjgl.opengl.GL30.GL_FRAMEBUFFER_BINDING;
import static org.lwjgl.opengl.GL30.glBindFramebuffer;

/**
 * The live window: through a ruin's window one sees its pocket, and through the pocket's way out the ruin, as they
 * are now.
 * <p>
 * The link between the two is a translation ({@link PortalLink}), so the other side is the same world seen from the
 * eye moved by the shift, with the same orientation. It is drawn as cheaply as the engine allows: the opaque blocks
 * only, in a buffer half the size of the screen, forward lit, with no shadow, water or sky pass; the chunks of the
 * other side are meshed once, and again only where a block changes; and nothing at all is drawn unless a window is
 * near and in sight. A near plane laid on the far face of the other window keeps out whatever stands between the
 * moved eye and it. The picture is then laid on the face of the window one looks at, under its swirl.
 * <p>
 * What a client does not have, it cannot show: a client on another machine is sent no chunk a thousand blocks away,
 * and sees the swirl alone.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class PortalRenderSystem extends BaseComponentSystem implements RenderSystem {

    /** How near a ruin's window must be for its pocket to be drawn in it. */
    static final float REACH = 40.0f;

    /** How much of the ruin around its window is drawn from inside the pocket, sideways, below and above. */
    private static final int RUIN_SPREAD = 48;
    private static final int RUIN_BELOW = 8;
    private static final int RUIN_ABOVE = 24;

    /** The picture's resolution against the screen's. */
    private static final int DIVISOR = 2;

    /** How much of the swirl still shows through the picture. */
    private static final float OPACITY = 0.92f;

    /**
     * How far the picture stands in front of the window's face. The swirl is drawn where its tile is opaque and
     * writes its depth there, on that very face: laid on it exactly, the picture would lose the depth test to it.
     */
    private static final float LIFT = 0.02f;

    /** Room round the window's centre for the frustum test: half its diagonal and a margin. */
    private static final float PANE_RADIUS = 2.5f;

    private static final String CHUNK_MATERIAL = "CoreRendering:chunk";
    private static final String PORTAL_MATERIAL = "HeroDungeons:portal";

    @In
    private Context context;

    @In
    private EntityManager entityManager;

    @In
    private WorldRenderer worldRenderer;

    @In
    private WorldProvider worldProvider;

    private CubeGeometry geometry;
    private PocketPlots plots;
    private PortalMeshes meshes;
    private FBO picture;
    private Mesh quad;
    private boolean enabled = true;
    private boolean diagnose;
    private String diagnosis = "";

    private final Matrix4f projection = new Matrix4f();
    private final Matrix4f modelView = new Matrix4f();
    private final Matrix4f model = new Matrix4f();
    private final Matrix3f normalMatrix = new Matrix3f();
    private final IntBuffer viewport = BufferUtils.createIntBuffer(16);
    private final IntBuffer pictureViewport = BufferUtils.createIntBuffer(16);
    private String lastStats = "aucune fenêtre dessinée";

    /** One window to draw this frame: the face one looks at, and where its other side is. */
    private record View(Vector3f[] pane, Vector3ic offset, List<Vector3ic> chunks, Vector4f clip, float distance) {
    }

    @Override
    public void renderAlphaBlend() {
        Camera camera = worldRenderer.getActiveCamera();
        if (!enabled || camera == null || geometry() == null) {
            idle();
            return;
        }
        long started = System.nanoTime();
        Vector3f eye = new Vector3f(camera.getPosition());
        View view = choose(eye);
        if (view == null) {
            idle();
            return;
        }
        meshes().want(view.chunks());
        meshes.update(System.currentTimeMillis());
        if (meshes.ready().isEmpty()) {
            return;
        }
        SphereProjection sphere = context.get(SphereProjection.class);
        Vector3f[] corners = new Vector3f[4];
        Vector3f centre = new Vector3f();
        for (int i = 0; i < 4; i++) {
            corners[i] = relative(sphere, view.pane()[i], eye);
            centre.add(corners[i]);
        }
        centre.mul(0.25f);
        if (!camera.getViewFrustum().testSphere(centre.x, centre.y, centre.z, PANE_RADIUS)) {
            lastStats = "fenêtre hors du champ";
            return;
        }

        int program = glGetInteger(GL_CURRENT_PROGRAM);
        int framebuffer = glGetInteger(GL_FRAMEBUFFER_BINDING);
        int activeTexture = glGetInteger(GL_ACTIVE_TEXTURE);
        boolean blend = glIsEnabled(GL_BLEND);
        boolean cull = glIsEnabled(GL_CULL_FACE);
        int cullMode = glGetInteger(GL_CULL_FACE_MODE);
        boolean depthTest = glIsEnabled(GL_DEPTH_TEST);
        boolean depthMask = glGetBoolean(GL_DEPTH_WRITEMASK);
        glGetIntegerv(GL_VIEWPORT, viewport);
        int width = viewport.get(2);
        int height = viewport.get(3);
        int[] textures = new int[2];
        for (int unit = 0; unit < textures.length; unit++) {
            glActiveTexture(GL_TEXTURE0 + unit);
            textures[unit] = glGetInteger(GL_TEXTURE_BINDING_2D);
        }

        int triangles = drawOtherSide(camera, eye, view, width, height);

        glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);
        glViewport(viewport.get(0), viewport.get(1), width, height);
        composite(camera, corners, width, height);

        for (int unit = 0; unit < textures.length; unit++) {
            glActiveTexture(GL_TEXTURE0 + unit);
            glBindTexture(GL_TEXTURE_2D, textures[unit]);
        }
        glActiveTexture(activeTexture);
        set(GL_BLEND, blend);
        set(GL_CULL_FACE, cull);
        glCullFace(cullMode);
        set(GL_DEPTH_TEST, depthTest);
        glDepthMask(depthMask);
        glUseProgram(program);

        lastStats = String.format("%d tronçons, %d triangles, %.2f ms de CPU, image %dx%d",
                meshes.ready().size(), triangles, (System.nanoTime() - started) / 1.0e6, width / DIVISOR,
                height / DIVISOR);
    }

    @Override
    public void shutdown() {
        if (meshes != null) {
            meshes.dispose();
            meshes = null;
        }
        if (picture != null) {
            picture.dispose();
            picture = null;
        }
    }

    @Command(shortDescription = "Allume ou éteint la fenêtre vivante, et dit ce qu'elle coûte",
            helpText = "Sans argument : ce que la dernière image a dessiné à travers la fenêtre. « on » ou « off » "
                    + "l'allume ou l'éteint, pour comparer.",
            requiredPermission = PermissionManager.NO_PERMISSION)
    public String fenetreVivante(@CommandParam(value = "etat", required = false) String state) {
        if ("on".equals(state)) {
            enabled = true;
        } else if ("diag".equals(state)) {
            diagnose = true;
            return "Diagnostic à la prochaine image : " + diagnosis;
        } else if ("off".equals(state)) {
            enabled = false;
            idle();
        }
        return (enabled ? "Fenêtre vivante allumée : " : "Fenêtre vivante éteinte : ") + lastStats;
    }

    /**
     * The window to draw: the pocket's way out when the eye is in its room, else the nearest ruin's window in reach.
     */
    private View choose(Vector3f eye) {
        Vector3i block = new Vector3i((int) Math.floor(eye.x), (int) Math.floor(eye.y), (int) Math.floor(eye.z));
        View best = null;
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            PocketComponent state = pocket.getComponent(PocketComponent.class);
            if (!state.fromRuin || !state.built || state.closing || state.plot < 0 || state.plot >= plots.count()) {
                continue;
            }
            PortalLink link = new PortalLink(state.ruinX, state.ruinZ, state.floorY, state.gateSide,
                    centre(state.plot));
            Vector3ic shift = link.shift();
            Vector3f axis = link.axis();
            if (PocketRoom.box(link.pocketCentre()).contains(block)) {
                int side = link.alongPocket(eye) >= 0 ? 1 : -1;
                Vector3f far = farFace(state.ruinX, state.ruinZ, RuinWindow.DISTANCE, axis, side);
                return new View(lifted(link.pocketPane(side), axis, side), new Vector3i(shift).negate(),
                        ruinChunks(state),
                        plane(axis, side, far), 0);
            }
            Vector3f middle = middle(link.ruinPane(1));
            float distance = middle.distance(eye);
            if (distance > REACH || best != null && best.distance() <= distance) {
                continue;
            }
            int side = link.alongRuin(eye) >= 0 ? 1 : -1;
            Vector3ic centre = link.pocketCentre();
            Vector3f far = farFace(centre.x(), centre.z(), PocketRoom.EXIT_DISTANCE, axis, side);
            best = new View(lifted(link.ruinPane(side), axis, side), new Vector3i(shift),
                    chunksOf(PocketRoom.box(centre)),
                    plane(axis, side, far), distance);
        }
        return best;
    }

    /** Draws the other side into the picture, as seen from the eye moved by the view's offset. */
    private int drawOtherSide(Camera camera, Vector3f eye, View view, int width, int height) {
        FBO target = picture(width / DIVISOR, height / DIVISOR);
        glBindFramebuffer(GL_FRAMEBUFFER, target.getId());
        glViewport(0, 0, target.width(), target.height());
        BackdropProvider backdrop = context.get(BackdropProvider.class);
        float daylight = backdrop == null ? 1.0f : backdrop.getDaylight();
        // Past the chunks drawn, a sky: a flat colour that follows the day, as the fog of a far horizon does.
        glClearColor(0.02f + 0.50f * daylight, 0.03f + 0.65f * daylight, 0.06f + 0.84f * daylight, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
        glDisable(GL_BLEND);
        glEnable(GL_DEPTH_TEST);
        glDepthMask(true);
        glEnable(GL_CULL_FACE);
        glCullFace(GL_BACK);

        Material chunk = Assets.getMaterial(CHUNK_MATERIAL).orElse(null);
        Texture atlas = Assets.getTexture("engine:terrain").orElse(null);
        Texture effects = Assets.getTexture("engine:effects").orElse(null);
        if (chunk == null || atlas == null || effects == null) {
            return 0;
        }
        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, atlas.getId());
        glActiveTexture(GL_TEXTURE0 + 1);
        glBindTexture(GL_TEXTURE_2D, effects.getId());
        glActiveTexture(GL_TEXTURE0);

        Vector3f moved = new Vector3f(eye).add(view.offset().x(), view.offset().y(), view.offset().z());
        Matrix4f viewMatrix = camera.getViewMatrix();
        projection.set(camera.getProjectionMatrix());
        FrustumIntersection frustum = new FrustumIntersection(new Matrix4f(projection).mul(viewMatrix));
        oblique(projection, viewMatrix, view.clip(), moved);

        chunk.activateFeature(ShaderProgramFeature.FEATURE_USE_FORWARD_LIGHTING);
        chunk.enable();
        chunk.setInt("textureAtlas", 0, true);
        chunk.setInt("textureEffects", 1, true);
        chunk.setFloat("daylight", daylight, true);
        chunk.setFloat("time", worldProvider.getTime().getDays(), true);
        if (backdrop != null) {
            chunk.setFloat3("sunVec", backdrop.getSunDirection(false), true);
        }
        chunk.setFloat("clip", -1.0e9f, true);
        // The other side is drawn as stored: the pocket is flat, and the ruin is flat enough round its window.
        chunk.setInt("sphereEnabled", 0, true);
        chunk.setInt("distantTerrain", 0, true);
        chunk.setInt("distantFade", 0, true);
        chunk.setMatrix4("projectionMatrix", projection, true);

        Vector4f clip = view.clip();
        Vector3f origin = new Vector3f();
        int triangles = 0;
        for (Map.Entry<Vector3ic, ChunkMesh> entry : meshes.ready().entrySet()) {
            Vector3ic position = entry.getKey();
            origin.set(position.x() * Chunks.SIZE_X, position.y() * Chunks.SIZE_Y, position.z() * Chunks.SIZE_Z);
            float minX = origin.x - moved.x;
            float minY = origin.y - moved.y;
            float minZ = origin.z - moved.z;
            if (!frustum.testAab(minX, minY, minZ, minX + Chunks.SIZE_X, minY + Chunks.SIZE_Y,
                    minZ + Chunks.SIZE_Z) || behind(clip, origin)) {
                continue;
            }
            ChunkMesh mesh = entry.getValue();
            mesh.updateMaterial(chunk, origin, false);
            model.translation(minX, minY, minZ);
            modelView.set(viewMatrix).mul(model);
            chunk.setMatrix4("modelViewMatrix", modelView, true);
            chunk.setMatrix3("normalMatrix", modelView.normal(normalMatrix), true);
            triangles += mesh.render(ChunkMesh.RenderPhase.OPAQUE);
        }
        // Every pass that draws the chunks bends them; the next to use this permutation expects it on.
        if (context.get(SphereProjection.class) != null) {
            chunk.setInt("sphereEnabled", 1, true);
        }
        chunk.deactivateFeature(ShaderProgramFeature.FEATURE_USE_FORWARD_LIGHTING);
        return triangles;
    }

    /** Lays the picture on the face of the window, at the screen position of each of its pixels. */
    private void composite(Camera camera, Vector3f[] corners, int width, int height) {
        Material portal = Assets.getMaterial(PORTAL_MATERIAL).orElse(null);
        if (portal == null) {
            diagnosis = "matériau du portail introuvable";
            return;
        }
        glEnable(GL_DEPTH_TEST);
        glDepthMask(false);
        glEnable(GL_BLEND);
        glDisable(GL_CULL_FACE);
        portal.enable();
        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, picture.getColorBufferTextureId());
        portal.setInt("portalView", 0, true);
        portal.setFloat2("invViewSize", 1.0f / width, 1.0f / height, true);
        portal.setFloat("opacity", OPACITY, true);
        for (int i = 0; i < 4; i++) {
            portal.setFloat3("corner" + i, corners[i], true);
        }
        portal.setMatrix4("projectionMatrix", camera.getProjectionMatrix(), true);
        portal.setMatrix4("modelViewMatrix", camera.getViewMatrix(), true);
        float[] before = diagnose ? centre(width, height) : null;
        quad().render();
        if (diagnose) {
            diagnose = false;
            float[] after = centre(width, height);
            int bound = glGetInteger(GL_FRAMEBUFFER_BINDING);
            glBindFramebuffer(GL_FRAMEBUFFER, picture.getId());
            float[] inside = new float[4];
            glReadPixels(picture.width() / 2, picture.height() / 2, 1, 1, GL_RGBA, GL_FLOAT, inside);
            diagnosis = String.format("écran avant %s, après %s, image %s, erreur GL %d, coins %s",
                    java.util.Arrays.toString(before), java.util.Arrays.toString(after),
                    java.util.Arrays.toString(inside), glGetError(), java.util.Arrays.toString(corners));
            glBindFramebuffer(GL_FRAMEBUFFER, bound);
        }
    }

    private static float[] centre(int width, int height) {
        float[] pixel = new float[4];
        glReadPixels(width / 2, height / 2, 1, 1, GL_RGBA, GL_FLOAT, pixel);
        return pixel;
    }

    /** Where a point of the world is drawn, from the eye: bent on the planet, as the chunks round it are. */
    private static Vector3f relative(SphereProjection sphere, Vector3fc world, Vector3fc eye) {
        return sphere == null ? new Vector3f(world).sub(eye) : sphere.curvedPosition(world, new Vector3f());
    }

    /**
     * Replaces the near plane by the given one, so that nothing between the eye and the window is drawn (Lengyel,
     * "Oblique View Frustum Depth Projection and Clipping"). The plane is in the world; the matrix draws positions
     * relative to the eye, in the axes of the view.
     */
    static void oblique(Matrix4f projection, Matrix4f view, Vector4f plane, Vector3fc eye) {
        Vector3f normal = view.transformDirection(new Vector3f(plane.x, plane.y, plane.z));
        float w = plane.x * eye.x() + plane.y * eye.y() + plane.z * eye.z() - plane.w;
        float qx = (Math.signum(normal.x) + projection.m20()) / projection.m00();
        float qy = (Math.signum(normal.y) + projection.m21()) / projection.m11();
        float qz = -1.0f;
        float qw = (1.0f + projection.m22()) / projection.m32();
        float scale = 2.0f / (normal.x * qx + normal.y * qy + normal.z * qz + w * qw);
        projection.m02(normal.x * scale);
        projection.m12(normal.y * scale);
        projection.m22(normal.z * scale + 1.0f);
        projection.m32(w * scale);
    }

    /** The plane through a point, kept on the side away from the eye: n·p ≥ w. */
    private static Vector4f plane(Vector3f axis, int side, Vector3f through) {
        Vector3f normal = new Vector3f(axis).mul(-side);
        return new Vector4f(normal, normal.dot(through));
    }

    /** The face of a window moved a little towards the eye, on the given side. */
    private static Vector3f[] lifted(Vector3f[] pane, Vector3f axis, int side) {
        for (Vector3f corner : pane) {
            corner.fma(LIFT * side, axis);
        }
        return pane;
    }

    /** A point on the far face of a window's slab, seen from the given side. */
    private static Vector3f farFace(int originX, int originZ, int distance, Vector3f axis, int side) {
        float along = distance - 0.5f * side;
        return new Vector3f(originX + axis.x * along, 0, originZ + axis.z * along);
    }

    /** Whether a whole chunk lies on the eye's side of the clip plane, and so behind the window. */
    private static boolean behind(Vector4f clip, Vector3f origin) {
        float highest = Float.NEGATIVE_INFINITY;
        for (int corner = 0; corner < 8; corner++) {
            float x = origin.x + ((corner & 1) == 0 ? 0 : Chunks.SIZE_X);
            float y = origin.y + ((corner & 2) == 0 ? 0 : Chunks.SIZE_Y);
            float z = origin.z + ((corner & 4) == 0 ? 0 : Chunks.SIZE_Z);
            highest = Math.max(highest, clip.x * x + clip.y * y + clip.z * z - clip.w);
        }
        return highest < 0;
    }

    private static Vector3f middle(Vector3f[] pane) {
        return new Vector3f(pane[0]).add(pane[1]).add(pane[2]).add(pane[3]).mul(0.25f);
    }

    private static List<Vector3ic> ruinChunks(PocketComponent state) {
        return chunksOf(new BlockRegion(
                state.ruinX - RUIN_SPREAD, state.floorY - RUIN_BELOW, state.ruinZ - RUIN_SPREAD,
                state.ruinX + RUIN_SPREAD, state.floorY + RUIN_ABOVE, state.ruinZ + RUIN_SPREAD));
    }

    /** The chunks a region of blocks touches. */
    static List<Vector3ic> chunksOf(BlockRegion region) {
        Vector3i low = Chunks.toChunkPos(new Vector3i(region.minX(), region.minY(), region.minZ()), new Vector3i());
        Vector3i high = Chunks.toChunkPos(new Vector3i(region.maxX(), region.maxY(), region.maxZ()), new Vector3i());
        List<Vector3ic> chunks = new ArrayList<>();
        for (int x = low.x; x <= high.x; x++) {
            for (int y = low.y; y <= high.y; y++) {
                for (int z = low.z; z <= high.z; z++) {
                    chunks.add(new Vector3i(x, y, z));
                }
            }
        }
        return chunks;
    }

    /** No window near: the meshes are let go, so that a window left behind costs nothing. */
    private void idle() {
        if (meshes != null) {
            meshes.want(Collections.emptyList());
        }
    }

    private static void set(int capability, boolean on) {
        if (on) {
            glEnable(capability);
        } else {
            glDisable(capability);
        }
    }

    private PortalMeshes meshes() {
        if (meshes == null) {
            meshes = new PortalMeshes(worldProvider);
        }
        return meshes;
    }

    private FBO picture(int width, int height) {
        if (picture == null || picture.width() != width || picture.height() != height) {
            if (picture != null) {
                picture.dispose();
            }
            picture = FBO.create(new SimpleUri("HeroDungeons:portalPicture"), new FBO.Dimensions(width, height),
                    FBO.Type.HDR, true, false, false, false);
        }
        return picture;
    }

    /** A unit square, which the shader stretches over the four corners of the window. */
    private Mesh quad() {
        if (quad == null) {
            StandardMeshData data = new StandardMeshData();
            Vector3f corner = new Vector3f();
            data.position.put(corner.set(0, 0, 0));
            data.position.put(corner.set(1, 0, 0));
            data.position.put(corner.set(1, 1, 0));
            data.position.put(corner.set(0, 1, 0));
            data.indices.put(0);
            data.indices.put(1);
            data.indices.put(2);
            data.indices.put(2);
            data.indices.put(3);
            data.indices.put(0);
            quad = Assets.generateAsset(data, Mesh.class);
        }
        return quad;
    }

    private Vector3i centre(int plot) {
        Vector2ic column = plots.centre(plot);
        return new Vector3i(column.x(), geometry.seaLevel(), column.y());
    }

    /** The cube's geometry, or null on a world that is not a cube and so has no pockets. */
    private CubeGeometry geometry() {
        if (geometry == null) {
            geometry = context.get(CubeGeometry.class);
            if (geometry == null && context.get(WorldGenerator.class) instanceof CubeWorldGenerator cube) {
                geometry = cube.getGeometry();
            }
            if (geometry != null) {
                plots = new PocketPlots(geometry.faceBlocks());
            }
        }
        return geometry;
    }
}
