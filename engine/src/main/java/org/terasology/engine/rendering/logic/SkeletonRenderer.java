// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0

package org.terasology.engine.rendering.logic;

import com.google.common.collect.Maps;
import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.lwjgl.BufferUtils;
import org.lwjgl.opengl.GL33;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.engine.config.Config;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnActivatedComponent;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.RenderSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.location.Location;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.CoreRegistry;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.assets.animation.MeshAnimation;
import org.terasology.engine.rendering.assets.animation.MeshAnimationFrame;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.assets.mesh.StandardMeshData;
import org.terasology.engine.rendering.assets.mesh.resource.AllocationType;
import org.terasology.engine.rendering.assets.mesh.resource.DrawingMode;
import org.terasology.engine.rendering.assets.skeletalmesh.Bone;
import org.terasology.engine.rendering.opengl.OpenGLSkeletalMesh;
import org.terasology.engine.rendering.sphere.SphereProjection;
import org.terasology.engine.rendering.world.WorldRenderer;
import org.terasology.engine.utilities.Assets;
import org.terasology.gestalt.assets.management.AssetManager;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.joml.geom.AABBf;
import org.terasology.nui.Color;

import java.nio.FloatBuffer;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Random;
import java.util.Set;

/**
 * Renders the skeletal mesh of entities when the debug setting "renderSkeletons" is active.
 * <p>
 * The entities must have a {@link SkeletalMeshComponent} and a {@link LocationComponent}.
 *
 * @see BoundingBoxRenderer another debug renderer for bounding boxes defined by shape components
 */
@RegisterSystem(RegisterMode.CLIENT)
public class SkeletonRenderer extends BaseComponentSystem implements RenderSystem, UpdateSubscriberSystem {

    private static final Logger logger = LoggerFactory.getLogger(SkeletonRenderer.class);

    @In
    private EntityManager entityManager;

    @In
    private WorldRenderer worldRenderer;

    @In
    private AssetManager assetManager;

    @In
    private Config config;

    private StandardMeshData meshData = new StandardMeshData(DrawingMode.LINES, AllocationType.STREAM);
    private Mesh mesh;
    private Material material;

    private Random random = new Random();

    /**
     * Null on a flat world, in which case nothing below changes. Resolved by hand, as MeshRenderer does: an absent
     * binding makes the injector log, and a flat world has nothing to report.
     */
    private SphereProjection sphereProjection;

    // Scratch for the pose of a bone, set for every bone of every creature every frame.
    private final Vector3f bonePos = new Vector3f();
    private final Quaternionf boneRot = new Quaternionf();
    private final Vector3f boneScale = new Vector3f();

    // Scratch for renderOpaque, which runs once a frame for every creature in sight.
    private final Quaternionf worldRot = new Quaternionf();
    private final Vector3f worldPos = new Vector3f();
    private final Vector3f offsetFromCamera = new Vector3f();
    private final Vector3f drawnAt = new Vector3f();
    private final Vector3f aabbCenter = new Vector3f();
    private final Vector3f scaledExtents = new Vector3f();
    private final AABBf aabb = new AABBf();
    private final Matrix3f surfaceFrame = new Matrix3f();
    private final Matrix3f normalMatrix = new Matrix3f();
    private final Matrix4f frameMatrix = new Matrix4f();
    private final Matrix4f entityMatrix = new Matrix4f();
    private final Matrix4f modelViewMatrix = new Matrix4f();
    private final FloatBuffer tempMatrixBuffer44 = BufferUtils.createFloatBuffer(16);
    private final FloatBuffer tempMatrixBuffer33 = BufferUtils.createFloatBuffer(12);
    private final FloatBuffer boneBuffer = BufferUtils.createFloatBuffer(16 * OpenGLSkeletalMesh.MAX_BONES);
    private final Set<Material> skinnedMaterials = new HashSet<>();
    /** Per bone: where it stands in the space of the entity, and the skinning matrix that follows from it. */
    private Matrix4f[] model = new Matrix4f[0];
    private Matrix4f[] skin = new Matrix4f[0];

    @Override
    public void initialise() {
        super.initialise();
        mesh = Assets.generateAsset(meshData, Mesh.class);
        sphereProjection = CoreRegistry.get(SphereProjection.class);
        material = assetManager.getAsset("engine:white", Material.class).get();
    }

    @ReceiveEvent(components = {SkeletalMeshComponent.class, LocationComponent.class})
    public void newSkeleton(OnActivatedComponent event, EntityRef entity) {
        SkeletalMeshComponent skeleton = entity.getComponent(SkeletalMeshComponent.class);
        if (skeleton.mesh == null) {
            return;
        }

        if (skeleton.boneEntities == null) {
            skeleton.boneEntities = Maps.newHashMap();
        }
        for (Bone bone : skeleton.mesh.getBones()) {
            EntityRef boneEntity = skeleton.boneEntities.get(bone.getName());
            // A saved boneEntities map can reference an entity that failed to restore (e.g. a
            // dangling id from an older/corrupted save) - recreate it rather than crash the whole
            // world load on what's otherwise a single skeleton's worth of missing bone state.
            if (boneEntity == null || !boneEntity.exists() || !boneEntity.hasComponent(LocationComponent.class)) {
                if (boneEntity != null && boneEntity.exists()) {
                    logger.warn("Bone entity for \"{}\" exists but has no LocationComponent, recreating", bone.getName());
                    // Destroy before replacing the map entry, or the old one is orphaned in the
                    // world - these accumulate on every load of an affected save.
                    boneEntity.destroy();
                }
                boneEntity = entityManager.create(new LocationComponent());
                skeleton.boneEntities.put(bone.getName(), boneEntity);
            }
        }

        for (Bone bone : skeleton.mesh.getBones()) {
            EntityRef boneEntity = skeleton.boneEntities.get(bone.getName());
            EntityRef parent = (bone.getParent() != null) ? skeleton.boneEntities.get(bone.getParent().getName()) : entity;
            Location.attachChild(parent, boneEntity);
        }
        for (Bone bone : skeleton.mesh.getBones()) {
            EntityRef boneEntity = skeleton.boneEntities.get(bone.getName());
            LocationComponent loc = boneEntity.getComponent(LocationComponent.class);
            loc.setLocalPosition(bone.getLocalPosition());
            loc.setLocalRotation(bone.getLocalRotation());
            loc.setLocalScale(bone.getLocalScale().x);
            boneEntity.saveComponent(loc);
            if (bone.getParent() == null) {
                skeleton.rootBone = boneEntity;
            }
        }
        entity.saveComponent(skeleton);
    }

    @Override
    public void update(float delta) {
        for (EntityRef entity : entityManager.getEntitiesWith(SkeletalMeshComponent.class, LocationComponent.class)) {
            updateSkeletalMeshOfEntity(entity, delta);
        }
    }

    private void updateSkeletalMeshOfEntity(EntityRef entity, float delta) {
        SkeletalMeshComponent skeletalMeshComp = entity.getComponent(SkeletalMeshComponent.class);
        if (skeletalMeshComp.mesh == null) {
            return;
        }

        // Saved only when the animation itself changes, which is replicated and which other systems read. What moves
        // every frame — the time into the animation, the pose of the bones — is written in place, as GaitClientSystem
        // writes animationRate: saving it copied the component into the save's delta recorder and offered it to the
        // network, for every creature and every bone, every frame, for data nobody sends and that is rebuilt from the
        // animation on load.
        MeshAnimation before = skeletalMeshComp.animation;
        if (skeletalMeshComp.animation == null && skeletalMeshComp.animationPool != null) {
            skeletalMeshComp.animation = randomAnimationData(skeletalMeshComp, random);
        }

        if (skeletalMeshComp.animation == null) {
            return;
        }

        if (skeletalMeshComp.animation.getFrameCount() < 1) {
            return;
        }
        skeletalMeshComp.animationTime += delta * skeletalMeshComp.animationRate;
        float animationDuration = getDurationOfAnimation(skeletalMeshComp);
        while (skeletalMeshComp.animationTime >= animationDuration) {
            MeshAnimation newAnimation;
            if (!skeletalMeshComp.loop) {
                newAnimation = null;
            } else if (skeletalMeshComp.animationPool != null && !skeletalMeshComp.animationPool.isEmpty()) {
                newAnimation = randomAnimationData(skeletalMeshComp, random);
            } else {
                newAnimation = skeletalMeshComp.animation;
            }

            if (newAnimation == null) {
                MeshAnimation finishedAnimation = skeletalMeshComp.animation;
                skeletalMeshComp.animationTime = animationDuration;
                MeshAnimationFrame frame = skeletalMeshComp.animation.getFrame(skeletalMeshComp.animation.getFrameCount() - 1);
                updateSkeleton(skeletalMeshComp, frame, frame, 1.0f);
                // Set animation to null so that AnimEndEvent fires only once
                skeletalMeshComp.animation = null;
                entity.saveComponent(skeletalMeshComp);
                entity.send(new AnimEndEvent(finishedAnimation));
                return;
            }
            skeletalMeshComp.animationTime -= animationDuration;
            if (skeletalMeshComp.animationTime < 0) {
                // In case the float calculation wasn't exact:
                skeletalMeshComp.animationTime = 0;
            }
            skeletalMeshComp.animation = newAnimation;
            animationDuration = getDurationOfAnimation(skeletalMeshComp);
        }
        float framePos = skeletalMeshComp.animationTime / skeletalMeshComp.animation.getTimePerFrame();
        int frameAId = (int) framePos;
        int frameBId = frameAId + 1;
        if (frameBId >= skeletalMeshComp.animation.getFrameCount()) {
            // In case the float calcuation wasn't exact:
            frameBId = skeletalMeshComp.animation.getFrameCount() - 1;
        }
        MeshAnimationFrame frameA = skeletalMeshComp.animation.getFrame(frameAId);
        MeshAnimationFrame frameB = skeletalMeshComp.animation.getFrame(frameBId);
        updateSkeleton(skeletalMeshComp, frameA, frameB, framePos - frameAId);
        if (skeletalMeshComp.animation != before) {
            entity.saveComponent(skeletalMeshComp);
        }
    }

    private float getDurationOfAnimation(SkeletalMeshComponent skeletalMeshComp) {
        return skeletalMeshComp.animation.getTimePerFrame() * (skeletalMeshComp.animation.getFrameCount() - 1);
    }

    private static MeshAnimation randomAnimationData(SkeletalMeshComponent skeletalMeshComp, Random random) {
        List<MeshAnimation> animationPool = skeletalMeshComp.animationPool;
        if (animationPool == null) {
            return null;
        }
        if (animationPool.isEmpty()) {
            return null;
        }
        return animationPool.get(random.nextInt(animationPool.size()));
    }

    private void updateSkeleton(SkeletalMeshComponent skeletalMeshComp, MeshAnimationFrame frameA, MeshAnimationFrame frameB,
                                float interpolationVal) {
        for (int i = 0; i < skeletalMeshComp.animation.getBoneCount(); ++i) {
            String boneName = skeletalMeshComp.animation.getBoneName(i);
            Bone bone = skeletalMeshComp.mesh.getBone(boneName);
            EntityRef boneEntity = skeletalMeshComp.boneEntities.get(boneName);
            if (boneEntity == null || bone == null) {
                continue;
            }
            LocationComponent boneLoc = boneEntity.getComponent(LocationComponent.class);
            if (boneLoc != null) {
                // In place, see updateSkeletalMeshOfEntity. A bone has a parent, so it never raises a
                // LocationChangedEvent either way, and whatever hangs from it reads its place through the chain.
                boneLoc.setLocalPosition(frameA.getPosition(i).lerp(frameB.getPosition(i), interpolationVal, bonePos));
                Quaternionf newRot = frameA.getRotation(i).slerp(frameB.getRotation(i), interpolationVal, boneRot);
                newRot.normalize();
                boneLoc.setLocalRotation(newRot);
                boneLoc.setLocalScale(frameA.getBoneScale(i).lerp(frameB.getBoneScale(i), interpolationVal, boneScale).x);
            }
        }
    }

    @Override
    public void renderOpaque() {
        Vector3fc cameraPosition = worldRenderer.getActiveCamera().getPosition();
        skinnedMaterials.clear();

        for (EntityRef entity : entityManager.getEntitiesWith(SkeletalMeshComponent.class, LocationComponent.class)) {

            SkeletalMeshComponent skeletalMesh = entity.getComponent(SkeletalMeshComponent.class);
            if (skeletalMesh.mesh == null
                    || skeletalMesh.material == null
                    || skeletalMesh.boneEntities == null
                    || !skeletalMesh.material.isRenderable()) {
                continue;
            }
            OpenGLSkeletalMesh mesh = (OpenGLSkeletalMesh) skeletalMesh.mesh;
            LocationComponent location = entity.getComponent(LocationComponent.class);
            location.getWorldRotation(worldRot);
            location.getWorldPosition(worldPos);
            float worldScale = location.getWorldScale();

            worldPos.sub(cameraPosition, offsetFromCamera);
            if (sphereProjection != null) {
                // Placed rigidly in the local frame at its own origin, as MeshRenderer places a static mesh: bent
                // vertex by vertex, a creature straddling a seam would tear.
                sphereProjection.curvedPosition(worldPos, offsetFromCamera);
                sphereProjection.frameAt(worldPos, surfaceFrame);
            }

            // The bind pose bounds always count. A glTF animation carries no bounds of its own - the loader hands out
            // AABBf(0, 0, 0) - so taking the animation's box alone would cull an animated character down to a single
            // point, and make it blink out at the edge of the screen the moment it started moving.
            aabb.set(skeletalMesh.mesh.getStaticAabb());
            MeshAnimation animation = skeletalMesh.animation;
            if (animation != null) {
                aabb.union(animation.getAabb());
            }
            // Tested where it is drawn, which on a sphere is not where it stands in the grid.
            drawnAt.set(cameraPosition).add(offsetFromCamera);
            aabb.transform(entityMatrix.translationRotateScale(drawnAt, worldRot, worldScale));

            //Scale bounding box for skeletalMesh.
            Vector3f scale = skeletalMesh.scale;

            aabb.center(aabbCenter);
            aabb.extent(scaledExtents).mul(scale.x(), scale.y(), scale.z());
            aabb.setMin(aabbCenter).setMax(aabbCenter).expand(scaledExtents);

            if (!worldRenderer.getActiveCamera().hasInSight(aabb)) {
                continue;
            }

            skeletalMesh.material.enable();
            skeletalMesh.material.setFloat("sunlight", 1.0f, true);
            skeletalMesh.material.setFloat("blockLight", 1.0f, true);
            skeletalMesh.material.setFloat3("colorOffset", skeletalMesh.color.rf(),
                    skeletalMesh.color.gf(), skeletalMesh.color.bf(), true);

            skeletalMesh.material.setMatrix4("projectionMatrix", worldRenderer.getActiveCamera().getProjectionMatrix());
            skeletalMesh.material.bindTextures();

            entityMatrix.translation(offsetFromCamera);
            if (sphereProjection != null) {
                entityMatrix.mul(frameMatrix.set(surfaceFrame));
            }
            entityMatrix.translate(0, skeletalMesh.heightOffset, 0).rotate(worldRot).scale(worldScale);
            boolean onGpu = mesh.isSkinnedOnGpu();
            if (onGpu) {
                // The CPU path applies these to the skinned vertices themselves.
                entityMatrix.translate(skeletalMesh.translate).scale(skeletalMesh.scale);
            }

            worldRenderer.getActiveCamera().getViewMatrix().mul(entityMatrix, modelViewMatrix);
            modelViewMatrix.get(tempMatrixBuffer44);
            skeletalMesh.material.setMatrix4("modelViewMatrix", tempMatrixBuffer44, true);

            modelViewMatrix.normal(normalMatrix).get(tempMatrixBuffer33);
            skeletalMesh.material.setMatrix3("normalMatrix", tempMatrixBuffer33, true);

            // Lit where it is, unless it has been told otherwise — see SkeletalMeshComponent.lightPosition.
            Vector3f lightPos = skeletalMesh.lightPosition == null ? worldPos : skeletalMesh.lightPosition;
            skeletalMesh.material.setFloat("sunlight", worldRenderer.getMainLightIntensityAt(lightPos), true);
            skeletalMesh.material.setFloat("blockLight", worldRenderer.getBlockLightIntensityAt(lightPos), true);

            int boneCount = pose(skeletalMesh, mesh);
            if (onGpu) {
                boneBuffer.clear();
                for (int i = 0; i < boneCount; i++) {
                    skin[i].get(16 * i, boneBuffer);
                }
                boneBuffer.limit(16 * boneCount);
                skeletalMesh.material.setMatrix4("boneMatrices", boneBuffer, true);
                skeletalMesh.material.setInt("boneCount", boneCount, true);
                skinnedMaterials.add(skeletalMesh.material);
                mesh.render();
            } else {
                skeletalMesh.material.setInt("boneCount", 0, true);
                Matrix4f[] transposed = new Matrix4f[boneCount];
                for (int i = 0; i < boneCount; i++) {
                    transposed[i] = skin[i].transpose(new Matrix4f());
                }
                mesh.setScaleTranslate(skeletalMesh.scale, skeletalMesh.translate);
                mesh.render(Arrays.asList(transposed));
            }
        }

        // A material is a program of its own, and its uniforms outlive the draw: one also used for a static mesh
        // must not skin it with the last creature's bones.
        for (Material material : skinnedMaterials) {
            material.setInt("boneCount", 0, true);
        }
    }

    /**
     * Fills {@link #skin} with the matrix of each bone, read from the bone entities. Built down the hierarchy, each
     * bone from its parent: asking every bone for its transform relative to the entity walked the whole chain above
     * it, one allocated matrix per link.
     *
     * @return the number of bones
     */
    private int pose(SkeletalMeshComponent skeletalMesh, OpenGLSkeletalMesh mesh) {
        int boneCount = mesh.getBones().size();
        if (skin.length < boneCount) {
            skin = grow(skin, boneCount);
            model = grow(model, boneCount);
        }
        mesh.skinMatrices((bone, dest) -> {
            EntityRef boneEntity = skeletalMesh.boneEntities.get(bone.getName());
            LocationComponent boneLocation = boneEntity == null ? null : boneEntity.getComponent(LocationComponent.class);
            if (boneLocation == null) {
                logger.warn("Unable to resolve bone \"{}\"", bone.getName()); //NOPMD
                return false;
            }
            dest.translationRotateScale(boneLocation.position, boneLocation.rotation, boneLocation.scale);
            return true;
        }, model, skin);
        return boneCount;
    }

    private static Matrix4f[] grow(Matrix4f[] matrices, int size) {
        Matrix4f[] grown = Arrays.copyOf(matrices, size);
        for (int i = matrices.length; i < size; i++) {
            grown[i] = new Matrix4f();
        }
        return grown;
    }

    @Override
    public void renderOverlay() {
        if (config.getRendering().getDebug().isRenderSkeletons()) {

            meshData.reallocate(0, 0);
            meshData.indices.rewind();
            meshData.position.rewind();
            meshData.color0.rewind();

            Vector3f cameraPosition = worldRenderer.getActiveCamera().getPosition();

            Matrix4f relMat = new Matrix4f();
            Matrix4f relFinal = new Matrix4f();
            Matrix4f entityTransform = new Matrix4f();

            Matrix4f result = new Matrix4f();
            Vector3f currentPos = new Vector3f();

            int index = 0;
            for (EntityRef entity : entityManager.getEntitiesWith(SkeletalMeshComponent.class, LocationComponent.class)) {
                SkeletalMeshComponent skeletalMesh = entity.getComponent(SkeletalMeshComponent.class);
                LocationComponent locationComponent = entity.getComponent(LocationComponent.class);
                if (skeletalMesh.boneEntities == null) {
                    continue;
                }

                Vector3f location = locationComponent.getWorldPosition(new Vector3f());
                Quaternionf rotation = locationComponent.getWorldRotation(new Quaternionf());
                entityTransform.translationRotateScale(location, rotation, 1.0f); // transformation of the entity

                // position is referenced around (0,0,0) (worldposition - cameraposition)
                Vector3f worldPositionCameraSpace = cameraPosition.negate(new Vector3f());

                // same heightOffset is applied to worldPositionCameraSpace from #renderOpaque()
                // TODO: resolve repeated logic for transformation applied to bones
                worldPositionCameraSpace.y += skeletalMesh.heightOffset;

                Matrix4f matrixCameraSpace = new Matrix4f().translationRotateScale(worldPositionCameraSpace, new Quaternionf(), 1.0f);
                Matrix4f modelViewMatrix = new Matrix4f(worldRenderer.getActiveCamera().getViewMatrix()).mul(matrixCameraSpace);
                material.setMatrix4("projectionMatrix", worldRenderer.getActiveCamera().getProjectionMatrix());
                material.setMatrix4("modelViewMatrix", modelViewMatrix, true);

                for (Bone bone : skeletalMesh.mesh.getBones()) {
                    Bone parentBone = bone.getParent();
                    EntityRef boneEntity = skeletalMesh.boneEntities.get(bone.getName());
                    if (parentBone == null) {
                        continue;
                    }

                    // TODO: the position of the bone is de-coupled from the actual translation/scale
                    EntityRef boneParentEntity = skeletalMesh.boneEntities.get(parentBone.getName());
                    LocationComponent locCompA = boneEntity.getComponent(LocationComponent.class);
                    LocationComponent locCompB = boneParentEntity.getComponent(LocationComponent.class);

                    // need to calculate the relative transformation from the entity to the start of the bone
                    locCompA.getRelativeTransform(relMat.identity(), entity);
                    // entityTransform * (scale, translation) * relativeMat * [x,y,z,1]
                    result.set(entityTransform)
                            .mul(relFinal.identity()
                                    .scale(skeletalMesh.scale)
                                    .translate(skeletalMesh.translate)
                                    .mul(relMat))
                            .transformPosition(currentPos.zero()); // get the position of the start of the bone
                    meshData.position.put(currentPos); // the start of the bone

                    // need to calculate the relative transformation from the entity to the connecting bone
                    locCompB.getRelativeTransform(relMat.identity(), entity);
                    // entityTransform * (scale, translation) * relativeMat * [x,y,z,1]
                    result.set(entityTransform)
                            .mul(relFinal
                                    .identity()
                                    .scale(skeletalMesh.scale)
                                    .translate(skeletalMesh.translate)
                                    .mul(relMat))
                            .transformPosition(currentPos.zero()); // get the position to the connecting bone
                    meshData.position.put(currentPos); // the end of the bone

                    meshData.color0.put(Color.white);
                    meshData.color0.put(Color.white);

                    meshData.indices.putAll(new int[]{
                            index, index + 1
                    });

                    index += 2;
                }

            }

            GL33.glDepthFunc(GL33.GL_ALWAYS);
            material.enable();
            mesh.reload(meshData);
            mesh.render();
            GL33.glDepthFunc(GL33.GL_LEQUAL);
        }
    }
}
