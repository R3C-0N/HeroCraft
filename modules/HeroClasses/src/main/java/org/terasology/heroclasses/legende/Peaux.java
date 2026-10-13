// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.assets.material.MaterialData;
import org.terasology.engine.rendering.assets.texture.Texture;
import org.terasology.engine.rendering.assets.texture.TextureData;
import org.terasology.engine.utilities.Assets;
import org.terasology.gestalt.assets.ResourceUrn;

import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.Map;

/**
 * Painted skins as engine assets, client side: a texture and the hero material that reads it, made once per
 * distinct appearance and shared by every character that looks the same.
 * <p>
 * The material is {@code engine:herosPeau} with another sheet — same shader, same alpha cut-out, which is what
 * lets layer 2, the beard and the hanging hair keep their silhouette.
 */
public final class Peaux {

    private static final Map<String, Material> MATERIAUX = new HashMap<>();
    private static int suivant;

    private Peaux() {
    }

    /** @return the material wearing this appearance's sheet */
    public static Material materiau(Apparence a) {
        String cle = cle(a);
        Material connu = MATERIAUX.get(cle);
        if (connu != null && !connu.isDisposed()) {
            return connu;
        }
        Material materiau = creer(Integer.toString(suivant++), Peintre.peindre(a));
        MATERIAUX.put(cle, materiau);
        return materiau;
    }

    /** @return the painted sheet as a texture, under a name of its own */
    public static Texture texture(String nom, Planche planche) {
        byte[] octets = planche.octets();
        ByteBuffer tampon = ByteBuffer.allocateDirect(octets.length);
        tampon.put(octets).flip();
        TextureData data = new TextureData(Planche.TAILLE, Planche.TAILLE, new ByteBuffer[]{tampon},
                Texture.WrapMode.CLAMP, Texture.FilterMode.NEAREST);
        return Assets.generateAsset(new ResourceUrn("HeroClasses", nom), data, Texture.class);
    }

    private static Material creer(String cle, Planche planche) {
        Texture texture = texture("peau" + cle, planche);
        MaterialData data = new MaterialData(Assets.getShader("engine:genericMeshMaterial").get());
        data.setParam("diffuse", texture);
        data.setParam("colorOffset", new float[]{1, 1, 1});
        data.setParam("textured", true);
        data.setParam("alphaThreshold", 0.5f);
        return Assets.generateAsset(new ResourceUrn("HeroClasses", "peau" + cle), data, Material.class);
    }

    /** Only what the painter reads: name, height and build do not change the sheet. */
    private static String cle(Apparence a) {
        return String.join("|", a.espece, Integer.toString(a.variante), a.peau, a.formeTete, a.yeux,
                a.couleurYeux, a.bouche, a.marques, a.coiffure, a.couleurCheveux, a.barbe, a.tenue, a.couleurTenue);
    }
}
