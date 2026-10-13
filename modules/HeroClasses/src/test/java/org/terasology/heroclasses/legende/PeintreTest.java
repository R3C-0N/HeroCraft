// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.junit.jupiter.api.Test;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.awt.image.Raster;
import java.io.IOException;
import java.io.InputStream;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

/**
 * The painter against the mock-up, texel for texel: the 100 reference sheets of
 * {@code tools/heros/maquette/references.mjs} (every sub-race by default, then 60 seeded draws), with the
 * configurations that painted them.
 */
public class PeintreTest {

    private static final String DOSSIER = "/legende/";

    @Test
    void everyReferenceSheetIsPaintedTexelForTexel() {
        Map<String, Object> configs = configs();
        assertEquals(100, configs.size());
        List<String> echecs = new ArrayList<>();
        for (Map.Entry<String, Object> entry : configs.entrySet()) {
            String nom = entry.getKey();
            Planche planche = Peintre.peindre(apparence(asMap(entry.getValue())));
            Raster attendu = png(nom + ".png").getRaster();
            assertEquals(4, attendu.getNumBands(), nom + ": not an RGBA sheet");
            String ecart = premierEcart(nom, attendu, planche);
            if (ecart != null) {
                echecs.add(ecart);
            }
        }
        if (!echecs.isEmpty()) {
            fail(echecs.size() + "/" + configs.size() + " sheets differ:\n" + String.join("\n", echecs));
        }
    }

    /** The 60 draws replay with the mock-up's seeded generator in place of {@code Math.random}. */
    @Test
    void randomAppearancesReplayTheMockUpsDraws() {
        Map<String, Object> configs = configs();
        Random lcg = new Lcg(12345);
        for (int n = 0; n < 60; n++) {
            String nom = String.format("tirage-%02d", n);
            Object attendu = configs.get(nom);
            assertNotNull(attendu, nom);
            assertApparence(nom, apparence(asMap(attendu)), Atlas.aleatoire(lcg));
        }
    }

    @Test
    void defaultsKeepWhatTheFirstSubRaceImposes() {
        Map<String, Object> configs = configs();
        for (String id : Atlas.ORDRE_ESPECES) {
            Apparence c = Atlas.defaut(id);
            Apparence attendu = apparence(asMap(configs.get("defaut-" + id + "-0")));
            assertTrue(Atlas.peauxDe(c).contains(c.peau), id);
            assertApparence(id, attendu, c);
        }
    }

    @Test
    void shadeAndMixRoundThroughHex() {
        assertEquals("#ca9b71", Peintre.shade("#E0AC7E", -0.1));
        assertEquals("#ffffff", Peintre.shade("#E0AC7E", 1));
        assertEquals("#858586", Peintre.shade("#0A0B0C", 0.5));
        assertEquals("#b9795c", Peintre.mix("#E0AC7E", "#8A3B33", 0.45));
    }

    private static void assertApparence(String nom, Apparence attendu, Apparence vu) {
        assertEquals(attendu.espece, vu.espece, nom + " espece");
        assertEquals(attendu.variante, vu.variante, nom + " variante");
        assertEquals(attendu.taille, vu.taille, nom + " taille");
        assertEquals(attendu.corpulence, vu.corpulence, nom + " corpulence");
        assertEquals(attendu.peau, vu.peau, nom + " peau");
        assertEquals(attendu.formeTete, vu.formeTete, nom + " formeTete");
        assertEquals(attendu.yeux, vu.yeux, nom + " yeux");
        assertEquals(attendu.couleurYeux, vu.couleurYeux, nom + " couleurYeux");
        assertEquals(attendu.bouche, vu.bouche, nom + " bouche");
        assertEquals(attendu.marques, vu.marques, nom + " marques");
        assertEquals(attendu.coiffure, vu.coiffure, nom + " coiffure");
        assertEquals(attendu.couleurCheveux, vu.couleurCheveux, nom + " couleurCheveux");
        assertEquals(attendu.barbe, vu.barbe, nom + " barbe");
        assertEquals(attendu.tenue, vu.tenue, nom + " tenue");
        assertEquals(attendu.couleurTenue, vu.couleurTenue, nom + " couleurTenue");
    }

    private static String premierEcart(String nom, Raster attendu, Planche planche) {
        int[] rgba = new int[4];
        for (int y = 0; y < Planche.TAILLE; y++) {
            for (int x = 0; x < Planche.TAILLE; x++) {
                attendu.getPixel(x, y, rgba);
                int voulu = rgba[0] << 24 | rgba[1] << 16 | rgba[2] << 8 | rgba[3];
                int peint = planche.rgba(x, y);
                if (voulu != peint) {
                    return String.format("%s: first difference at (%d, %d), expected %08x, painted %08x",
                            nom, x, y, voulu, peint);
                }
            }
        }
        return null;
    }

    private static Apparence apparence(Map<String, Object> json) {
        Apparence c = new Apparence();
        c.nom = (String) json.get("nom");
        c.espece = (String) json.get("espece");
        c.variante = ((Double) json.get("variante")).intValue();
        c.taille = (Double) json.get("taille");
        c.corpulence = (Double) json.get("corpulence");
        c.peau = (String) json.get("peau");
        c.formeTete = (String) json.get("formeTete");
        c.yeux = (String) json.get("yeux");
        c.couleurYeux = (String) json.get("couleurYeux");
        c.bouche = (String) json.get("bouche");
        c.marques = (String) json.get("marques");
        c.coiffure = (String) json.get("coiffure");
        c.couleurCheveux = (String) json.get("couleurCheveux");
        c.barbe = (String) json.get("barbe");
        c.tenue = (String) json.get("tenue");
        c.couleurTenue = (String) json.get("couleurTenue");
        return c;
    }

    /* ---------- resources ---------- */

    private static Map<String, Object> configs() {
        try (InputStream in = PeintreTest.class.getResourceAsStream(DOSSIER + "configs.json")) {
            assertNotNull(in, "configs.json is missing from the test resources");
            return asMap(new Json(new String(in.readAllBytes(), StandardCharsets.UTF_8)).lire());
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    private static BufferedImage png(String fichier) {
        try (InputStream in = PeintreTest.class.getResourceAsStream(DOSSIER + fichier)) {
            assertNotNull(in, fichier + " is missing from the test resources");
            return ImageIO.read(in);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> asMap(Object value) {
        return (Map<String, Object>) value;
    }

    /** The mock-up's seeded {@code Math.random}: a 31-bit LCG computed in doubles, as JavaScript does. */
    private static final class Lcg extends Random {
        private double graine;

        Lcg(double graine) {
            this.graine = graine;
        }

        @Override
        public double nextDouble() {
            graine = (graine * 1103515245 + 12345) % 2147483648.0;
            return graine / 2147483648.0;
        }
    }

    /** Just enough JSON for configs.json: objects, arrays, strings, numbers (as doubles), booleans, null. */
    private static final class Json {
        private final String texte;
        private int i;

        Json(String texte) {
            this.texte = texte;
        }

        Object lire() {
            blancs();
            char c = texte.charAt(i);
            if (c == '{') {
                Map<String, Object> map = new LinkedHashMap<>();
                i++;
                blancs();
                if (texte.charAt(i) == '}') {
                    i++;
                    return map;
                }
                while (true) {
                    blancs();
                    String cle = chaine();
                    blancs();
                    attendre(':');
                    map.put(cle, lire());
                    blancs();
                    if (texte.charAt(i++) == '}') {
                        return map;
                    }
                }
            }
            if (c == '[') {
                List<Object> list = new ArrayList<>();
                i++;
                blancs();
                if (texte.charAt(i) == ']') {
                    i++;
                    return list;
                }
                while (true) {
                    list.add(lire());
                    blancs();
                    if (texte.charAt(i++) == ']') {
                        return list;
                    }
                }
            }
            if (c == '"') {
                return chaine();
            }
            if (texte.startsWith("true", i)) {
                i += 4;
                return Boolean.TRUE;
            }
            if (texte.startsWith("false", i)) {
                i += 5;
                return Boolean.FALSE;
            }
            if (texte.startsWith("null", i)) {
                i += 4;
                return null;
            }
            int debut = i;
            while (i < texte.length() && "+-0123456789.eE".indexOf(texte.charAt(i)) >= 0) {
                i++;
            }
            return Double.parseDouble(texte.substring(debut, i));
        }

        private String chaine() {
            attendre('"');
            StringBuilder sb = new StringBuilder();
            while (true) {
                char c = texte.charAt(i++);
                if (c == '"') {
                    return sb.toString();
                }
                if (c != '\\') {
                    sb.append(c);
                    continue;
                }
                char e = texte.charAt(i++);
                switch (e) {
                    case 'n' -> sb.append('\n');
                    case 't' -> sb.append('\t');
                    case 'r' -> sb.append('\r');
                    case 'b' -> sb.append('\b');
                    case 'f' -> sb.append('\f');
                    case 'u' -> {
                        sb.append((char) Integer.parseInt(texte.substring(i, i + 4), 16));
                        i += 4;
                    }
                    default -> sb.append(e);
                }
            }
        }

        private void attendre(char c) {
            if (texte.charAt(i++) != c) {
                throw new IllegalStateException("expected '" + c + "' at " + (i - 1));
            }
        }

        private void blancs() {
            while (i < texte.length() && Character.isWhitespace(texte.charAt(i))) {
                i++;
            }
        }
    }
}
