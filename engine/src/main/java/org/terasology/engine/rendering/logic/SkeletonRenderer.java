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
import org.terasology.engine.entitySystem.entity.lifecycleEvents.BeforeDeactivateComponent;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnActivatedComponent;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.RenderSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.location.Location;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.assets.animation.MeshAnimation;
import org.terasology.engine.rendering.assets.animation.MeshAnimationFrame;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.assets.mesh.StandardMeshData;
import org.terasology.engine.rendering.assets.mesh.resource.AllocationType;
import org.terasology.engine.rendering.assets.mesh.resource.DrawingMode;
import org.terasology.engine.rendering.assets.skeletalmesh.Bone;
import org.terasology.engine.rendering.assets.skeletalmesh.SkeletalMesh;
import org.terasology.engine.rendering.opengl.OpenGLSkeletalMesh;
import org.terasology.engine.rendering.world.WorldRenderer;
import org.terasology.engine.utilities.Assets;
import org.terasology.gestalt.assets.management.AssetManager;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.joml.geom.AABBf;
import org.terasology.nui.Color;

import java.nio.FloatBuffer;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

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

    /** Which bone entity each channel of an animation drives, per skeleton — see {@link BoneChannels}. */
    private final Map<EntityRef, BoneChannels> channels = new HashMap<>();
    private final Vector3f bonePosition = new Vector3f();
    private final Quaternionf boneRotation = new Quaternionf();
    private final Vector3f boneScale = new Vector3f();

    @Override
    public void initialise() {
        super.initialise();
        mesh = Assets.generateAsset(meshData, Mesh.class);
        material = assetManager.getAsset("engine:white", Material.class).get();
    }

    @ReceiveEvent(components = {SkeletalMeshComponent.class, LocationComponent.class})
    public void newSkeleton(OnActivatedComponent event, EntityRef entity) {
        // The bone map below may be refilled in place, which an identity check would not notice.
        channels.remove(entity);
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

    @ReceiveEvent(components = SkeletalMeshComponent.class)
    public void forgetSkeleton(BeforeDeactivateComponent event, EntityRef entity) {
        channels.remove(entity);
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

        MeshAnimation playing = skeletalMeshComp.animation;
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
                updateSkeleton(entity, skeletalMeshComp, frame, frame, 1.0f);
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
        updateSkeleton(entity, skeletalMeshComp, frameA, frameB, framePos - frameAId);
        // Only a change of animation is worth telling anyone about. The clock alone moves every frame, is not
        // replicated, and saving for it copied the whole component — bone map included — into the pending save
        // and flagged it dirty for every connected client, once per animated entity per frame.
        if (skeletalMeshComp.animation != playing) {
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

    private void updateSkeleton(EntityRef entity, SkeletalMeshComponent skeletalMeshComp, MeshAnimationFrame frameA,
                                MeshAnimationFrame frameB, float interpolationVal) {
        EntityRef[] bones = channelsOf(entity, skeletalMeshComp).bones;
        for (int i = 0; i < bones.length; ++i) {
            if (bones[i] == null) {
                continue;
            }
            LocationComponent boneLoc = bones[i].getComponent(LocationComponent.class);
            if (boneLoc != null) {
                boneLoc.setLocalPosition(frameA.getPosition(i).lerp(frameB.getPosition(i), interpolationVal, bonePosition));
                boneLoc.setLocalRotation(frameA.getRotation(i).slerp(frameB.getRotation(i), interpolationVal, boneRotation)
                        .normalize());
                boneLoc.setLocalScale(frameA.getBoneScale(i).lerp(frameB.getBoneScale(i), interpolationVal, boneScale).x);
                // Not saved, on purpose. A bone is local to this client, nothing listens for its changes, and
                // the pose is recomputed from the animation every frame: saving it only copied each bone into the
                // pending save game, seven times per creature per frame.
            }
        }
    }

    private BoneChannels channelsOf(EntityRef entity, SkeletalMeshComponent skeletalMeshComp) {
        BoneChannels bound = channels.get(entity);
        if (bound == null || !bound.isFor(skeletalMeshComp)) {
            bound = new BoneChannels(skeletalMeshComp);
            channels.put(entity, bound);
        }
        return bound;
    }

    /**
     * The bone entity behind each channel of an animation, resolved once.
     * <p>
     * An animation names its bones and a skeleton keeps its bone entities by name, so posing a frame used to cost two
     * string-keyed lookups per bone per frame before any arithmetic. The answer only changes when the mesh, the
     * animation or the bone map does, and those three are compared by identity each frame instead. The entities are
     * kept, not their components: the component is fetched afresh every frame, so a bone that is restored from a save
     * is never animated through a stale copy.
     */
    private static final class BoneChannels {
        private final SkeletalMesh mesh;
        private final MeshAnimation animation;
        private final Map<String, EntityRef> boneEntities;
        private final EntityRef[] bones;

        BoneChannels(SkeletalMeshComponent skeletalMeshComp) {
            mesh = skeletalMeshComp.mesh;
            animation = skeletalMeshComp.animation;
            boneEntities = skeletalMeshComp.boneEntities;
            bones = new EntityRef[animation.getBoneCount()];
            for (int i = 0; i < bones.length; ++i) {
                String boneName = animation.getBoneName(i);
                if (mesh.getBone(boneName) != null && boneEntities != null) {
                    bones[i] = boneEntities.get(boneName);
                }
            }
        }

        boolean isFor(SkeletalMeshComponent skeletalMeshComp) {
            return mesh == skeletalMeshComp.mesh && animation == skeletalMeshComp.animation
                    && boneEntities == skeletalMeshComp.boneEntities;
        }
    }

    @Override
    public void renderOpaque() {
        Vector3fc cameraPosition = worldRenderer.getActiveCamera().getPosition();

        Quaternionf worldRot = new Quaternionf();
        Vector3f worldPos = new Vector3f();
        Quaternionf inverseWorldRot = new Quaternionf();

        FloatBuffer tempMatrixBuffer44 = BufferUtils.createFloatBuffer(16);
        FloatBuffer tempMatrixBuffer33 = BufferUtils.createFloatBuffer(12);

        for (EntityRef entity : entityManager.getEntitiesWith(SkeletalMeshComponent.class, LocationComponent.class)) {

            SkeletalMeshComponent skeletalMesh = entity.getComponent(SkeletalMeshComponent.class);
            if (skeletalMesh.mesh == null
                    || skeletalMesh.material == null
                    || skeletalMesh.boneEntities == null
                    || !skeletalMesh.material.isRenderable()) {
                continue;
            }
            // The bind pose bounds always count. A glTF animation carries no bounds of its own - the loader hands out
            // AABBf(0, 0, 0) - so taking the animation's box alone would cull an animated character down to a single
            // point, and make it blink out at the edge of the screen the moment it started moving.
            AABBf aabb = new AABBf(skeletalMesh.mesh.getStaticAabb());
            MeshAnimation animation = skeletalMesh.animation;
            if (animation != null) {
                aabb.union(animation.getAabb());
            }
            LocationComponent location = entity.getComponent(LocationComponent.class);
            location.getWorldRotation(worldRot);
            worldRot.invert(inverseWorldRot);
            location.getWorldPosition(worldPos);
            float worldScale = location.getWorldScale();

            aabb = aabb.transform(new Matrix4f().translationRotateScale(worldPos, worldRot, worldScale), new AABBf());

            //Scale bounding box for skeletalMesh.
            Vector3f scale = skeletalMesh.scale;

            Vector3f aabbCenter = aabb.center(new Vector3f());
            Vector3f scaledExtents = aabb.extent(new Vector3f()).mul(scale.x(), scale.y(), scale.z());
            aabb = new AABBf(aabbCenter, aabbCenter).expand(scaledExtents);

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

            Vector3f worldPositionCameraSpace = new Vector3f();
            worldPos.sub(cameraPosition, worldPositionCameraSpace);

            worldPositionCameraSpace.y += skeletalMesh.heightOffset;
            Matrix4f matrixCameraSpace = new Matrix4f().translationRotateScale(worldPositionCameraSpace, worldRot, worldScale);

            Matrix4f modelViewMatrix = worldRenderer.getActiveCamera().getViewMatrix().mul(matrixCameraSpace, new Matrix4f());
            modelViewMatrix.get(tempMatrixBuffer44);
            skeletalMesh.material.setMatrix4("modelViewMatrix", tempMatrixBuffer44, true);

            modelViewMatrix.normal(new Matrix3f()).get(tempMatrixBuffer33);
            skeletalMesh.material.setMatrix3("normalMatrix", tempMatrixBuffer33, true);

            // Lit where it is, unless it has been told otherwise — see SkeletalMeshComponent.lightPosition.
            Vector3f lightPos = skeletalMesh.lightPosition == null ? worldPos : skeletalMesh.lightPosition;
            skeletalMesh.material.setFloat("sunlight", worldRenderer.getMainLightIntensityAt(lightPos), true);
            skeletalMesh.material.setFloat("blockLight", worldRenderer.getBlockLightIntensityAt(lightPos), true);

            Matrix4f[] boneTransforms = new Matrix4f[skeletalMesh.mesh.getBones().size()];
            for (Bone bone : skeletalMesh.mesh.getBones()) {
                EntityRef boneEntity = skeletalMesh.boneEntities.get(bone.getName());
                if (boneEntity == null) {
                    boneEntity = EntityRef.NULL;
                }
                LocationComponent boneLocation = boneEntity.getComponent(LocationComponent.class);
                if (boneLocation != null) {
                    Matrix4f boneTransform = new Matrix4f();
                    boneLocation.getRelativeTransform(boneTransform, entity);
                    boneTransform.mul(bone.getInverseBindMatrix());
                    boneTransforms[bone.getIndex()] = boneTransform.transpose();
                } else {
                    logger.warn("Unable to resolve bone \"{}\"", bone.getName()); //NOPMD
                    boneTransforms[bone.getIndex()] = new Matrix4f();
                }
            }

            ((OpenGLSkeletalMesh) skeletalMesh.mesh).setScaleTranslate(skeletalMesh.scale, skeletalMesh.translate);
            ((OpenGLSkeletalMesh) skeletalMesh.mesh).render(Arrays.asList(boneTransforms));
        }
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
