// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import java.util.List;
import java.util.Map;

/**
 * Paints a hero's skin on the 64x64 two-layer pattern, procedurally, from its {@link Apparence}.
 * <p>
 * <strong>A port, texel for texel.</strong> The reference is the mock-up {@code tools/heros/maquette/hero-atlas.js}
 * and its canvas shim; the test compares every sheet with the mock-up's. Arithmetic is therefore kept as the
 * mock-up writes it: doubles, the same order of operations, {@link StrictMath#sin} for the noise (V8's sine is the
 * same fdlibm), and every colour rounded through {@code #rrggbb} as the mock-up's {@code shade} and {@code mix} do.
 * Colours are carried as {@code 0xRRGGBB} ints between those roundings.
 * <p>
 * Wings, beard, hair, cranial appendages and tail are painted into the free squares of the same sheet.
 */
public final class Peintre {

    private static final List<String> MEMBRES1 = List.of("tete", "torse", "brasD", "brasG", "jambeD", "jambeG");
    private static final List<String> BRAS = List.of("brasD", "brasG");
    private static final List<String> JAMBES = List.of("jambeD", "jambeG");
    private static final List<String> COTES = List.of("devant", "derriere", "droite", "gauche");
    private static final int OS = 0xEDE7D6;

    /** Cranial appendages: per row of the 8x8 square, the {x, width} spans; both sides share the square. */
    private static final Map<String, int[][][]> APPENDICES = Map.of(
            "oreillesFelin", new int[][][] {{}, {}, {}, {{0, 2}, {6, 2}}, {{0, 2}, {6, 2}}, {{0, 3}, {5, 3}},
                {{1, 2}, {5, 2}}, {{1, 2}, {5, 2}}},
            "oreillesLupin", new int[][][] {{{0, 2}, {6, 2}}, {{0, 2}, {6, 2}}, {{0, 2}, {6, 2}}, {{0, 3}, {5, 3}},
                {{0, 3}, {5, 3}}, {{1, 3}, {4, 3}}, {{1, 2}, {5, 2}}, {{1, 2}, {5, 2}}},
            "oreillesVulpin", new int[][][] {{{1, 2}, {5, 2}}, {{0, 3}, {5, 3}}, {{0, 3}, {5, 3}}, {{0, 3}, {5, 3}},
                {{0, 3}, {5, 3}}, {{1, 3}, {4, 3}}, {{1, 2}, {5, 2}}, {{2, 2}, {4, 2}}},
            "cornes", new int[][][] {{}, {{0, 2}, {6, 2}}, {{0, 2}, {6, 2}}, {{1, 2}, {5, 2}}, {{1, 2}, {5, 2}},
                {{1, 2}, {5, 2}}, {{2, 2}, {4, 2}}, {{2, 2}, {4, 2}}},
            "cornesCourtes", new int[][][] {{}, {}, {}, {}, {{0, 2}, {6, 2}}, {{1, 2}, {5, 2}}, {{1, 2}, {5, 2}},
                {{2, 2}, {4, 2}}},
            "bois", new int[][][] {{{0, 1}, {7, 1}}, {{0, 1}, {2, 1}, {5, 1}, {7, 1}}, {{1, 1}, {2, 1}, {5, 1}, {6, 1}},
                {{1, 2}, {5, 2}}, {{2, 1}, {5, 1}}, {{2, 1}, {5, 1}}, {{2, 2}, {4, 2}}, {{3, 2}}},
            "couronneOs", new int[][][] {{}, {{0, 1}, {7, 1}}, {{0, 1}, {3, 2}, {7, 1}}, {{0, 1}, {3, 2}, {7, 1}},
                {{0, 2}, {3, 2}, {6, 2}}, {{0, 2}, {3, 2}, {6, 2}}, {{0, 8}}, {{0, 8}}},
            "cristaux", new int[][][] {{{1, 1}, {6, 1}}, {{1, 2}, {5, 2}}, {{1, 2}, {3, 2}, {5, 2}},
                {{2, 1}, {3, 2}, {5, 1}}, {{2, 2}, {4, 2}}, {{3, 2}}, {{3, 2}}, {{2, 4}}},
            "couronne", new int[][][] {{}, {}, {{0, 1}, {2, 1}, {5, 1}, {7, 1}}, {{0, 1}, {2, 1}, {5, 1}, {7, 1}},
                {{0, 8}}, {{0, 8}}, {}, {}});

    /** Material of an appendage: ears, absent here, take the skin; {@code accent} is the sub-race's accent. */
    private static final Map<String, String> MAT_APPENDICE = Map.of(
            "cornes", "#D8CAA8", "cornesCourtes", "#CBBA96", "bois", "#8A6A44",
            "couronneOs", "#E8DFC8", "cristaux", "accent", "couronne", "accent");

    /** Tails: per row, the {x, width} span. */
    private static final Map<String, int[][]> QUEUES = Map.of(
            "longue", new int[][] {{3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}},
            "touffue", new int[][] {{3, 2}, {3, 3}, {2, 4}, {2, 4}, {2, 4}, {2, 4}, {3, 3}, {3, 2}},
            "panache", new int[][] {{3, 2}, {2, 4}, {2, 4}, {1, 6}, {1, 6}, {2, 4}, {2, 4}, {3, 2}});

    /** Hanging hair: per row, the {x, width} span, empty for none. */
    private static final Map<String, int[][]> CHUTES = Map.of(
            "mi", new int[][] {{1, 6}, {1, 6}, {1, 6}, {2, 4}, {}, {}, {}, {}},
            "longue", new int[][] {{1, 6}, {1, 6}, {1, 6}, {1, 6}, {1, 6}, {2, 4}, {2, 4}, {3, 2}},
            "fleuve", new int[][] {{0, 8}, {0, 8}, {1, 6}, {1, 6}, {1, 6}, {1, 6}, {2, 4}, {2, 4}},
            "queue", new int[][] {{3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}},
            "tresse", new int[][] {{3, 2}, {3, 2}, {3, 2}, {3, 2}, {3, 2}, {4, 1}, {3, 2}, {4, 1}},
            "nattes", new int[][] {{1, 2}, {1, 2}, {1, 2}, {1, 2}, {1, 2}, {1, 2}, {2, 1}, {2, 1}},
            "couettes", new int[][] {{0, 2}, {0, 2}, {0, 2}, {1, 1}, {}, {}, {}, {}},
            "mulet", new int[][] {{}, {}, {}, {1, 6}, {1, 6}, {2, 4}, {2, 4}, {3, 2}});

    /** Hanging styles painted twice, mirrored. */
    private static final List<String> MIROIR_CHUTE = List.of("nattes", "couettes");

    private Peintre() {
    }

    /* ---------- couleurs ---------- */

    /** @return the {@code #rrggbb} colour as {@code 0xRRGGBB} */
    static int rgb(String hex) {
        return Integer.parseInt(hex.substring(1), 16) & 0xFFFFFF;
    }

    private static int canal(double v) {
        return (int) Math.max(0, Math.min(255, Math.round(v)));
    }

    /** Lightens towards white ({@code amt > 0}) or darkens towards black ({@code amt < 0}), then rounds. */
    static int shade(int c, double amt) {
        int r = c >> 16 & 255;
        int g = c >> 8 & 255;
        int b = c & 255;
        return canal(amt >= 0 ? r + (255 - r) * amt : r * (1 + amt)) << 16
                | canal(amt >= 0 ? g + (255 - g) * amt : g * (1 + amt)) << 8
                | canal(amt >= 0 ? b + (255 - b) * amt : b * (1 + amt));
    }

    /** Blends from {@code a} to {@code b} by {@code t}, then rounds. */
    static int mix(int a, int b, double t) {
        int ar = a >> 16 & 255;
        int ag = a >> 8 & 255;
        int ab = a & 255;
        return canal(ar + ((b >> 16 & 255) - ar) * t) << 16
                | canal(ag + ((b >> 8 & 255) - ag) * t) << 8
                | canal(ab + ((b & 255) - ab) * t);
    }

    /** @return {@code hex} lightened or darkened by {@code amt}, as {@code #rrggbb} */
    public static String shade(String hex, double amt) {
        return hex(shade(rgb(hex), amt));
    }

    /** @return {@code a} blended towards {@code b} by {@code t}, as {@code #rrggbb} */
    public static String mix(String a, String b, double t) {
        return hex(mix(rgb(a), rgb(b), t));
    }

    private static String hex(int c) {
        String digits = Integer.toHexString(c | 0x1000000);
        return "#" + digits.substring(1);
    }

    /** Hash noise in [0, 1), the mock-up's to the last bit. */
    static double bruit(int x, int y, int s) {
        double v = StrictMath.sin((x + 1) * 12.9898 + (y + 1) * 78.233 + s * 3.77) * 43758.5453;
        return v - Math.floor(v);
    }

    /**
     * {@code Number(a.toFixed(2))}: the multiple of 0.01 nearest to the exact value of {@code a}, the larger one on
     * a tie, which does happen: 0.92 - 0.045 is exactly 0.875. The product is split with a fused multiply-add so
     * that the comparison with the midpoint is exact.
     */
    static double auCentieme(double a) {
        if (a < 0) {
            return -auCentieme(-a);
        }
        double p = a * 100;
        double reste = Math.fma(a, 100, -p);
        double n = Math.floor(p);
        if (p == n && reste < 0) {
            n -= 1;
        }
        boolean auDessus = p - (n + 0.5) >= -reste;
        return (n + (auDessus ? 1 : 0)) / 100;
    }

    /* ---------- verite a la JavaScript ---------- */

    private static boolean vrai(Double v) {
        return v != null && v != 0 && !v.isNaN();
    }

    private static boolean vrai(Integer v) {
        return v != null && v != 0;
    }

    private static boolean vrai(Boolean v) {
        return v != null && v;
    }

    private static boolean vrai(String v) {
        return v != null && !v.isEmpty();
    }

    /* ---------- pinceaux ---------- */

    private static Rect face(String part, String face) {
        return Atlas.PATRON.get(part).face(face);
    }

    /** A brush bound to one face: coordinates are local, and a rectangle that overflows the face is dropped. */
    private static final class Brosse {
        final Toile g;
        final Rect r;

        Brosse(Toile g, Rect r) {
            this.g = g;
            this.r = r;
        }

        Brosse(Toile g, String part, String face) {
            this(g, face(part, face));
        }

        void plein(int col) {
            g.fillRect(r.x(), r.y(), r.largeur(), r.hauteur(), col);
        }

        void px(int x, int y, int col) {
            px(x, y, col, 1, 1, 1);
        }

        void px(int x, int y, int col, int w, int h) {
            px(x, y, col, 1, w, h);
        }

        void px(int x, int y, int col, double alpha, int w, int h) {
            w = w != 0 ? w : 1;
            h = h != 0 ? h : 1;
            if (x < 0 || y < 0 || x + w > r.largeur() || y + h > r.hauteur()) {
                return;
            }
            g.fillRect(r.x() + x, r.y() + y, w, h, col, alpha);
        }

        /** The same block and its horizontal mirror on the face. */
        void sym(int x, int y, int col) {
            sym(x, y, col, 1, 1);
        }

        void sym(int x, int y, int col, int w, int h) {
            w = w != 0 ? w : 1;
            h = h != 0 ? h : 1;
            px(x, y, col, w, h);
            px(r.largeur() - x - w, y, col, w, h);
        }
    }

    /** A face filled with grain and fixed lighting: lit top edge, shaded bottom edge. */
    private static void plaque(Toile g, Rect r, int col, int seed, double amp) {
        g.fillRect(r.x(), r.y(), r.largeur(), r.hauteur(), col);
        int sombre = shade(col, -amp);
        int clair = shade(col, amp * 0.9);
        for (int y = 0; y < r.hauteur(); y++) {
            for (int x = 0; x < r.largeur(); x++) {
                double n = bruit(x + r.x(), y + r.y(), seed);
                if (n < 0.2) {
                    g.fillRect(r.x() + x, r.y() + y, 1, 1, sombre);
                } else if (n > 0.88) {
                    g.fillRect(r.x() + x, r.y() + y, 1, 1, clair);
                }
            }
        }
        g.fillRect(r.x(), r.y(), r.largeur(), 1, shade(col, amp * 1.4));
        g.fillRect(r.x(), r.y() + r.hauteur() - 1, r.largeur(), 1, shade(col, -amp * 1.6));
    }

    /* ---------- peinture ---------- */

    /** @return the skin of that appearance */
    public static Planche peindre(Apparence cfg) {
        Espece sp = Atlas.espece(cfg.espece);
        Traits t = Atlas.traitsDe(cfg);
        Toile g = new Toile();

        int peau = rgb(cfg.peau);
        int peauF = shade(peau, -0.1);
        int cheveux = rgb(cfg.couleurCheveux);
        double relief = vrai(t.relief()) ? t.relief() : 1;

        g.globalAlpha = vrai(t.alpha()) ? t.alpha() : 1;

        /* 1 -- peau */
        for (int i = 0; i < MEMBRES1.size(); i++) {
            for (int j = 0; j < Atlas.FACES.size(); j++) {
                String f = Atlas.FACES.get(j);
                int base = f.equals("droite") || f.equals("gauche") ? peauF
                        : f.equals("dessous") ? shade(peau, -0.18) : peau;
                plaque(g, face(MEMBRES1.get(i), f), base, i * 7 + j, vrai(t.pelage()) ? 0.13 * t.pelage() : 0.06);
            }
        }
        if (vrai(t.pelage())) {
            pelage(g, peau, t);
        }
        if (vrai(t.motif())) {
            motif(g, t.motif(), peau, vrai(t.accent()) ? rgb(t.accent()) : shade(peau, 0.18));
        }
        if (vrai(t.reflet())) {
            reflets(g, rgb(t.reflet()));
        }
        if (vrai(t.veines())) {
            veines(g, t);
        }

        /* 2 -- visage */
        visage(g, cfg, t, peau, cheveux, relief);

        /* 3 -- tenue, puis les marques qui se posent par-dessus */
        tenue(g, cfg, t, peau);
        marques(g, cfg, peau);

        /* 4 -- pilosite : uniquement des pieces de couche 2, jamais sur la peau */
        peindreBarbe(g, vrai(t.barbeForcee()) ? t.barbeForcee() : cfg.barbe, cheveux);
        peindreChevelure(g, cfg.coiffure, cheveux, t);

        /* 5 -- couche 2 */
        g.globalAlpha = 1;
        couche2(g, cfg, t, cheveux, peau);
        if (vrai(sp.mods().ailes())) {
            peindreAiles(g, rgb(vrai(t.reflet()) ? t.reflet() : "#E8C46A"));
        }
        return new Planche(g.px);
    }

    /** Spectral wing in texels: feathers fade towards the tip; the inner edge (x = 0) touches the back. */
    private static void peindreAiles(Toile g, int teinte) {
        int[] larg = {7, 7, 7, 6, 6, 5, 5, 4, 4, 3, 3, 2, 2, 1};
        List<String> faces = List.of("devant", "derriere");
        for (int k = 0; k < faces.size(); k++) {
            Brosse b = new Brosse(g, "aile", faces.get(k));
            for (int y = 0; y < 14; y++) {
                int w = larg[y];
                for (int x = 0; x < w; x++) {
                    boolean bord = x == w - 1;
                    boolean plume = (x + y) % 3 == 0;
                    double a = auCentieme(0.92 - y * 0.045 - x * 0.03);
                    /* the back face mirrors the front: it is the one seen on the right wing, turned 180 degrees */
                    double l = bord ? 0.42 : plume ? 0.2 : k != 0 ? 0 : 0.08;
                    b.px(k != 0 ? 6 - x : x, y, eclaircir(teinte, l), a, 1, 1);
                }
            }
            b.px(k != 0 ? 5 : 0, 0, eclaircir(teinte, 0.5), 0.95, 2, 1);
            b.px(k != 0 ? 6 : 0, 1, eclaircir(teinte, 0.34), 0.9, 1, 3);
        }
    }

    /** The wing's {@code rgba()} channels: towards white by {@code l}, rounded without clamping. */
    private static int eclaircir(int c, double l) {
        int r = c >> 16 & 255;
        int g = c >> 8 & 255;
        int b = c & 255;
        return (int) Math.round(r + (255 - r) * l) << 16 | (int) Math.round(g + (255 - g) * l) << 8
                | (int) Math.round(b + (255 - b) * l);
    }

    private static void pelage(Toile g, int peau, Traits t) {
        int clair = shade(peau, 0.18);
        int sombre = shade(peau, -0.22);
        double seuil = 0.86 - 0.08 * ((vrai(t.pelage()) ? t.pelage() : 1) - 1);
        String[][] zones = {{"torse", "devant"}, {"torse", "derriere"}, {"brasD", "devant"}, {"brasG", "devant"},
            {"jambeD", "devant"}, {"jambeG", "devant"}};
        for (int i = 0; i < zones.length; i++) {
            Brosse b = new Brosse(g, zones[i][0], zones[i][1]);
            for (int y = 0; y < b.r.hauteur(); y++) {
                for (int x = 0; x < b.r.largeur(); x++) {
                    double n = bruit(x * 3 + i, y * 2, 11);
                    if (n > seuil) {
                        b.px(x, y, sombre);
                    } else if (n < 0.14) {
                        b.px(x, y, clair);
                    }
                }
            }
        }
        Brosse b = new Brosse(g, "torse", "devant");
        for (int y = 5; y < 12; y++) {
            b.px(3, y, clair, 2, 1);
        }
    }

    /* ---------- motifs corporels de sous-race ----------
       Each pattern covers the skull and the hands, which no outfit hides: the sub-race stays readable even in
       plate armour. */

    private static void motif(Toile g, String motif, int peau, int acc) {
        switch (motif) {
            case "rayures" -> rayures(g, acc);
            case "selle" -> selle(g, peau, acc);
            case "contreOmbre" -> contreOmbre(g, peau, acc);
            case "front" -> front(g, peau, acc);
            case "taches" -> taches(g, acc);
            case "joues" -> joues(g, acc);
            case "poussiere" -> poussiere(g, acc);
            case "plisse" -> plisse(g, acc);
            case "suie" -> suie(g, acc);
            case "runes" -> runes(g, acc);
            case "lichen" -> lichen(g, acc);
            case "dorure" -> dorure(g, acc);
            case "givre" -> givre(g, acc);
            case "ecailles" -> ecailles(g, acc);
            case "craquelures" -> craquelures(g, acc);
            default -> {
            }
        }
    }

    private static void rayures(Toile g, int c) {
        int[][] torse = {{1, 0, 1, 5}, {4, 0, 1, 4}, {6, 1, 1, 5}, {2, 6, 1, 4}, {5, 7, 1, 4}};
        for (String f : List.of("devant", "derriere")) {
            Brosse b = new Brosse(g, "torse", f);
            for (int[] o : torse) {
                b.px(o[0], o[1], c, o[2], o[3]);
            }
        }
        for (String f : List.of("droite", "gauche")) {
            Brosse b = new Brosse(g, "torse", f);
            b.px(1, 1, c, 1, 4);
            b.px(2, 7, c, 1, 3);
        }
        for (String p : List.of("brasD", "brasG", "jambeD", "jambeG")) {
            int[] rangs = p.startsWith("bras") ? new int[] {1, 4, 7} : new int[] {0, 3, 6};
            for (String f : COTES) {
                Brosse b = new Brosse(g, p, f);
                for (int y : rangs) {
                    b.px(0, y, c, 4, 1);
                }
            }
        }
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.sym(1, 0, c, 1, 2);
        bt.sym(0, 3, c, 1, 1);
        for (String f : List.of("droite", "gauche")) {
            Brosse b = new Brosse(g, "tete", f);
            b.px(2, 1, c, 1, 3);
            b.px(5, 2, c, 1, 3);
        }
        new Brosse(g, "tete", "dessus").px(2, 0, c, 1, 8);
        new Brosse(g, "tete", "derriere").px(3, 0, c, 2, 5);
    }

    private static void selle(Toile g, int peau, int acc) {
        int s = shade(peau, -0.32);
        new Brosse(g, "torse", "derriere").px(0, 0, s, 8, 6);
        for (String f : List.of("droite", "gauche")) {
            new Brosse(g, "torse", f).px(0, 0, s, 4, 5);
        }
        new Brosse(g, "torse", "devant").px(2, 0, acc, 4, 4);
        for (String p : BRAS) {
            for (String f : COTES) {
                new Brosse(g, p, f).px(0, 0, s, 4, 4);
            }
        }
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.sym(0, 0, s, 3, 2);
        bt.px(2, 5, shade(peau, -0.18), 4, 3);
        new Brosse(g, "tete", "dessus").px(0, 0, s, 8, 5);
        new Brosse(g, "tete", "derriere").px(0, 0, s, 8, 5);
    }

    private static void contreOmbre(Toile g, int peau, int acc) {
        int noir = shade(peau, -0.62);
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.px(2, 5, acc, 4, 3);
        bt.sym(0, 4, acc, 1, 2);
        for (String f : List.of("devant", "derriere")) {
            new Brosse(g, "torse", f).px(2, 4, acc, 4, 8);
        }
        for (String p : List.of("brasD", "brasG", "jambeD", "jambeG")) {
            for (String f : COTES) {
                new Brosse(g, p, f).px(0, 8, noir, 4, 4);
            }
            new Brosse(g, p, "dessous").plein(noir);
        }
        new Brosse(g, "tete", "dessus").px(0, 0, shade(peau, -0.2), 8, 3);
    }

    private static void front(Toile g, int peau, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.px(1, 0, acc, 6, 2);
        bt.px(2, 5, shade(peau, 0.14), 4, 1);
        new Brosse(g, "tete", "dessus").px(1, 3, acc, 6, 5);
        for (String f : List.of("devant", "derriere")) {
            new Brosse(g, "torse", f).px(1, 0, shade(peau, 0.1), 6, 2);
        }
        for (String p : List.of("brasD", "brasG", "jambeD", "jambeG")) {
            for (String f : COTES) {
                new Brosse(g, p, f).px(0, 10, shade(peau, -0.5), 4, 2);
            }
        }
    }

    /* Pigment, wear, engraving: patterns that all sit on the skull and the hands, the only skin no outfit hides. */

    private static void taches(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        for (int[] o : new int[][] {{1, 4}, {2, 5}, {5, 4}, {6, 5}, {2, 3}, {5, 3}, {1, 6}, {6, 6}}) {
            bt.px(o[0], o[1], acc);
        }
        for (String p : BRAS) {
            Brosse b = new Brosse(g, p, "devant");
            for (int[] o : new int[][] {{1, 3}, {2, 5}, {0, 6}, {3, 8}}) {
                b.px(o[0], o[1], acc);
            }
        }
    }

    private static void joues(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.sym(1, 5, acc, 2, 1);
        bt.px(3, 4, acc, 2, 1);
        for (String f : List.of("droite", "gauche")) {
            new Brosse(g, "tete", f).px(3, 5, acc, 2, 1);
        }
    }

    private static void poussiere(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.px(1, 0, acc, 6, 1);
        bt.sym(0, 2, acc, 1, 1);
        new Brosse(g, "tete", "dessus").px(1, 1, acc, 6, 3);
        for (String f : List.of("devant", "derriere")) {
            new Brosse(g, "torse", f).px(0, 0, acc, 8, 1);
        }
    }

    private static void plisse(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.px(1, 1, acc, 6, 1);
        bt.sym(1, 2, acc, 2, 1);
        bt.sym(0, 5, acc, 2, 1);
        bt.sym(1, 6, acc, 1, 1);
    }

    private static void suie(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.sym(0, 5, acc, 2, 1);
        bt.px(2, 2, acc);
        bt.px(5, 3, acc);
        for (String p : BRAS) {
            for (String f : COTES) {
                new Brosse(g, p, f).px(0, 9, acc, 4, 3);
            }
        }
    }

    private static void runes(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.px(3, 1, acc);
        bt.px(4, 2, acc);
        bt.sym(1, 1, acc);
        for (String p : BRAS) {
            Brosse b = new Brosse(g, p, "devant");
            for (int[] o : new int[][] {{1, 4, 2, 1}, {2, 5, 1, 1}, {1, 6, 2, 1}, {1, 7, 1, 1}}) {
                b.px(o[0], o[1], acc, o[2], o[3]);
            }
        }
        new Brosse(g, "torse", "devant").px(3, 2, acc, 2, 1);
    }

    private static void lichen(Toile g, int acc) {
        String[][] zones = {{"tete", "devant"}, {"tete", "droite"}, {"tete", "gauche"}, {"torse", "devant"},
            {"brasD", "devant"}, {"brasG", "devant"}, {"jambeD", "devant"}, {"jambeG", "devant"}};
        for (int i = 0; i < zones.length; i++) {
            Brosse b = new Brosse(g, zones[i][0], zones[i][1]);
            for (int y = 0; y < b.r.hauteur(); y++) {
                for (int x = 0; x < b.r.largeur(); x++) {
                    if (bruit(x * 4 + i * 2, y * 3, 67) > 0.88) {
                        b.px(x, y, acc);
                    }
                }
            }
        }
    }

    private static void dorure(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.px(1, 0, acc, 6, 1);
        bt.sym(0, 4, acc, 1, 1);
        bt.sym(1, 2, acc, 1, 1);
        for (String f : List.of("devant", "derriere")) {
            Brosse b = new Brosse(g, "torse", f);
            b.px(0, 0, acc, 8, 1);
            b.px(3, 1, acc, 2, 1);
        }
        for (String p : BRAS) {
            new Brosse(g, p, "devant").px(0, 0, acc, 4, 1);
        }
    }

    private static void givre(Toile g, int acc) {
        Brosse bt = new Brosse(g, "tete", "devant");
        bt.sym(0, 1, acc, 1, 2);
        bt.sym(1, 0, acc, 1, 1);
        new Brosse(g, "tete", "dessus").px(0, 0, acc, 8, 2);
        for (String f : List.of("devant", "derriere")) {
            new Brosse(g, "torse", f).px(0, 0, acc, 8, 1);
        }
        for (String p : BRAS) {
            new Brosse(g, p, "dessus").plein(acc);
        }
    }

    private static void ecailles(Toile g, int acc) {
        String[][] zones = {{"tete", "devant"}, {"tete", "droite"}, {"tete", "gauche"}, {"torse", "devant"},
            {"torse", "derriere"}, {"brasD", "devant"}, {"brasG", "devant"}};
        for (int i = 0; i < zones.length; i++) {
            Brosse b = new Brosse(g, zones[i][0], zones[i][1]);
            for (int y = 0; y < b.r.hauteur(); y++) {
                for (int x = y % 2; x < b.r.largeur(); x += 2) {
                    if (bruit(x + i, y, 71) > 0.45) {
                        b.px(x, y, acc);
                    }
                }
            }
        }
    }

    private static void craquelures(Toile g, int acc) {
        String[][] zones = {{"tete", "devant"}, {"tete", "droite"}, {"tete", "gauche"}, {"torse", "devant"},
            {"brasD", "devant"}, {"brasG", "devant"}, {"jambeD", "devant"}, {"jambeG", "devant"}};
        for (int i = 0; i < zones.length; i++) {
            Brosse b = new Brosse(g, zones[i][0], zones[i][1]);
            int largeur = b.r.largeur();
            for (int y = 0; y < b.r.hauteur(); y++) {
                double n = bruit(y * 3 + i, 0, 83);
                if (n <= 0.6) {
                    continue;
                }
                int x = (int) Math.floor(n * largeur) % largeur;
                b.px(x, y, acc);
                b.px((x + 1) % largeur, y, acc);
            }
        }
    }

    private static void reflets(Toile g, int col) {
        g.save();
        g.globalAlpha *= 0.32;
        String[][] zones = {{"torse", "devant"}, {"torse", "derriere"}, {"brasD", "devant"}, {"brasG", "devant"},
            {"tete", "devant"}};
        for (String[] z : zones) {
            Brosse b = new Brosse(g, z[0], z[1]);
            for (int x = 0; x < b.r.largeur(); x++) {
                b.px(x, 1, col);
            }
            b.px(0, 2, col, b.r.largeur(), 1);
        }
        g.restore();
    }

    private static void veines(Toile g, Traits t) {
        int enc = vrai(t.craquelee()) ? 0x211B1B : 0x2A2426;
        boolean dense = t.veines() > 1;
        String[][] zones = dense
                ? new String[][] {{"tete", "devant"}, {"torse", "devant"}, {"brasD", "devant"}, {"brasG", "devant"},
                    {"torse", "derriere"}, {"jambeD", "devant"}, {"jambeG", "devant"}, {"tete", "droite"},
                    {"tete", "gauche"}}
                : new String[][] {{"tete", "devant"}, {"torse", "devant"}, {"brasD", "devant"}, {"brasG", "devant"}};
        for (int i = 0; i < zones.length; i++) {
            Brosse b = new Brosse(g, zones[i][0], zones[i][1]);
            for (int y = 0; y < b.r.hauteur(); y++) {
                for (int x = 0; x < b.r.largeur(); x++) {
                    double n = bruit(x * 5 + i * 3, y * 7, 23);
                    if (n > (dense ? 0.9 : 0.95)) {
                        b.px(x, y, enc);
                    }
                }
            }
        }
    }

    private static void visage(Toile g, Apparence cfg, Traits t, int peau, int cheveux, double relief) {
        Brosse b = new Brosse(g, "tete", "devant");
        int creux = shade(peau, -0.13 * relief);
        int creuxF = shade(peau, -0.24 * relief);

        /* forme de tete : ou tombe l'ombre */
        if ("ovale".equals(cfg.formeTete)) {
            b.sym(0, 0, creux);
            b.sym(0, 7, creux);
            b.sym(0, 1, creux);
        } else if ("anguleuse".equals(cfg.formeTete)) {
            b.sym(0, 5, creux);
            b.sym(0, 6, creuxF);
            b.sym(1, 7, creux);
        } else {
            b.sym(0, 6, creux, 1, 2);
        }
        if (vrai(t.machoire()) || vrai(t.machoireLourde())) {
            b.px(1, 7, creuxF, 6, 1);
            if (vrai(t.machoireLourde())) {
                b.px(0, 6, creuxF, 8, 1);
            }
        }

        /* nez */
        b.px(3, 4, creux, 2, 1);
        b.px(3, 5, creuxF, 2, 1);

        /* sourcils */
        b.sym(1, 2, shade(cheveux, -0.2), 2, 1);

        /* yeux */
        Expression spec = Atlas.yeux(cfg.yeux);
        if (spec == null) {
            spec = Atlas.YEUX.get(0);
        }
        boolean lumineux = "lumineux".equals(t.regard());
        boolean vitreux = "vitreux".equals(t.regard());
        int iris = lumineux ? rgb(vrai(t.lueur()) ? t.lueur() : "#F2DDA4") : vitreux ? 0xD8D4CE : rgb(cfg.couleurYeux);
        int blanc = lumineux || vitreux ? shade(iris, 0.25) : 0xF4F1EA;
        int paupiere = shade(peau, -0.3);
        int cil = shade(cheveux, -0.3);
        List<Touche> ops = lumineux || vitreux ? Atlas.yeux("vides").ops() : spec.ops();
        for (Touche o : ops) {
            int col = switch (o.teinte()) {
                case "b" -> blanc;
                case "i" -> iris;
                case "l" -> paupiere;
                case "c" -> cil;
                default -> throw new IllegalStateException("eye tint " + o.teinte());
            };
            b.sym(o.x(), o.y(), col, o.largeur(), o.hauteur());
        }

        /* bouche */
        int lev = mix(peau, 0x8A3B33, 0.45);
        Expression bouche = Atlas.bouche(cfg.bouche);
        if (bouche == null) {
            bouche = Atlas.BOUCHES.get(0);
        }
        for (Touche o : bouche.ops()) {
            int col = switch (o.teinte()) {
                case "l" -> lev;
                case "f" -> shade(lev, -0.3);
                case "d" -> OS;
                case "p" -> shade(peau, -0.12);
                default -> throw new IllegalStateException("mouth tint " + o.teinte());
            };
            b.px(o.x(), o.y(), col, o.largeur(), o.hauteur());
        }

        /* canines */
        if (vrai(t.canines())) {
            b.sym(2, 7, OS, 1, 1);
            if (t.canines() > 1) {
                b.sym(2, 6, OS, 1, 1);
            }
        }

        /* touffes laterales de l'homme-bete */
        if (vrai(t.pelage())) {
            for (String f : List.of("droite", "gauche")) {
                new Brosse(g, "tete", f).px(3, 1, shade(peau, -0.2), 2, 1);
            }
        }
    }

    private static Rect pieceDeMarque(String piece) {
        return switch (piece) {
            case "tete" -> face("tete", "devant");
            case "teteD" -> face("tete", "droite");
            case "teteG" -> face("tete", "gauche");
            case "torse" -> face("torse", "devant");
            default -> null;
        };
    }

    private static void marques(Toile g, Apparence cfg, int peau) {
        Marque spec = Atlas.marque(cfg.marques);
        if (spec == null || spec.ops().isEmpty()) {
            return;
        }
        int col = "peauF".equals(spec.teinte()) ? shade(peau, -0.38) : rgb(spec.teinte());
        for (Pose o : spec.ops()) {
            if (o.piece().equals("bras")) {
                for (String p : BRAS) {
                    new Brosse(g, p, "devant").px(o.x(), o.y(), col, o.largeur(), o.hauteur());
                }
                continue;
            }
            Rect cible = pieceDeMarque(o.piece());
            if (cible != null) {
                new Brosse(g, cible).px(o.x(), o.y(), col, o.largeur(), o.hauteur());
            }
        }
    }

    private static void tenue(Toile g, Apparence cfg, Traits t, int peau) {
        Tenue spec = Atlas.tenue(cfg.tenue);
        if (spec == null) {
            spec = Atlas.TENUES.get(0);
        }
        int base = rgb(cfg.couleurTenue);
        int fer = 0x98A1A8;
        int cuir = 0x5A3B25;
        int cuirF = 0x3E2818;
        int botte = 0x3F2A1C;
        int corps = switch (spec.ton()) {
            case "sombre" -> shade(base, -0.24);
            case "clair" -> shade(base, 0.16);
            case "plaque" -> mix(base, fer, 0.4);
            default -> base;
        };
        int trim = shade(corps, 0.16);
        int pantalon = mix(base, 0x2A2320, 0.5);

        if (!spec.torseNu()) {
            for (int j = 0; j < Atlas.FACES.size(); j++) {
                plaque(g, face("torse", Atlas.FACES.get(j)), corps, 40 + j, 0.07);
            }
            if (spec.col()) {
                for (String f : COTES) {
                    Brosse b = new Brosse(g, "torse", f);
                    b.px(0, 0, trim, b.r.largeur(), 1);
                }
            }
        }
        if (spec.ceinture()) {
            for (String f : COTES) {
                Brosse b = new Brosse(g, "torse", f);
                b.px(0, 8, cuir, b.r.largeur(), 2);
                b.px(0, 9, cuirF, b.r.largeur(), 1);
            }
            new Brosse(g, "torse", "devant").px(3, 8, fer, 2, 2);
        }
        if (!spec.extras().isEmpty()) {
            Brosse b = new Brosse(g, "torse", "devant");
            for (Touche o : spec.extras()) {
                int col = switch (o.teinte()) {
                    case "fer" -> fer;
                    case "clair" -> trim;
                    case "sombre" -> shade(corps, -0.26);
                    case "base" -> corps;
                    case "cuir" -> cuir;
                    case "peau" -> peau;
                    default -> corps;
                };
                b.px(o.x(), o.y(), col, o.largeur(), o.hauteur());
            }
        }

        /* bras : manches puis mains */
        for (int i = 0; i < BRAS.size(); i++) {
            String p = BRAS.get(i);
            if (spec.manche() > 0) {
                for (int j = 0; j < Atlas.FACES.size(); j++) {
                    String f = Atlas.FACES.get(j);
                    if (f.equals("dessous")) {
                        continue;
                    }
                    Rect r = face(p, f);
                    int h = f.equals("dessus") ? r.hauteur() : Math.min(spec.manche(), r.hauteur());
                    plaque(g, new Rect(r.x(), r.y(), r.largeur(), h), corps, 60 + i * 5 + j, 0.07);
                }
            }
            int main = vrai(t.mainsMassives()) ? 3 : 2;
            for (String f : COTES) {
                Brosse bb = new Brosse(g, p, f);
                bb.px(0, 12 - main, shade(peau, -0.05), bb.r.largeur(), main);
            }
            if (vrai(t.griffes())) {
                Brosse b = new Brosse(g, p, "devant");
                b.px(0, 11, OS, 1, 1);
                b.px(3, 11, OS, 1, 1);
            }
        }

        /* jambes */
        boolean robe = spec.jambes().equals("robe");
        int fin = robe ? 10 : 8;
        for (int i = 0; i < JAMBES.size(); i++) {
            String p = JAMBES.get(i);
            for (int j = 0; j < Atlas.FACES.size(); j++) {
                String f = Atlas.FACES.get(j);
                if (f.equals("dessous")) {
                    continue;
                }
                Rect r = face(p, f);
                int h = f.equals("dessus") ? r.hauteur() : fin;
                plaque(g, new Rect(r.x(), r.y(), r.largeur(), h), robe ? corps : pantalon, 80 + i * 5 + j, 0.06);
            }
            for (String f : List.of("devant", "derriere", "droite", "gauche", "dessous")) {
                Rect r = face(p, f);
                boolean dessous = f.equals("dessous");
                int haut = dessous ? 0 : fin;
                int h = dessous ? r.hauteur() : 12 - fin;
                plaque(g, new Rect(r.x(), r.y() + haut, r.largeur(), h), botte, 95 + i, 0.05);
            }
            new Brosse(g, p, "devant").px(0, fin, shade(botte, 0.18), 4, 1);
        }
    }

    /** Hair texel with its grain: lighter or darker by the noise. */
    private static void poseCheveu(Brosse b, int x, int y, int col) {
        double n = bruit(x * 3 + y, y * 5, 61);
        b.px(x, y, n > 0.86 ? shade(col, 0.1) : n < 0.2 ? shade(col, -0.16) : col);
    }

    /**
     * The hairline follows a profile per column, never a straight cut. Sides run front to back; the right side is
     * reversed.
     */
    private static void colonnes(Toile g, String face, int[] profil, int col, boolean inverse, int plafond) {
        if (profil == null) {
            return;
        }
        Brosse b = new Brosse(g, "tete2", face);
        for (int x = 0; x < 8; x++) {
            int index = inverse ? 7 - x : x;
            int h = Math.min(8, Math.min(plafond != 0 ? plafond : 8, index < profil.length ? profil[index] : 0));
            for (int y = 0; y < h; y++) {
                poseCheveu(b, x, y, col);
            }
        }
    }

    private static void couche2(Toile g, Apparence cfg, Traits t, int cheveux, int peau) {
        Coiffure co = Atlas.coiffure(cfg.coiffure);
        if (co == null) {
            co = Atlas.COIFFURES.get(0);
        }
        int volume = vrai(t.chevelureEtheree()) ? shade(cheveux, 0.2) : cheveux;
        int ombre = shade(volume, -0.12);

        int plafond = 8;
        if (co.der() != null && co.der().length > 0) {
            plafond = Integer.MAX_VALUE;
            for (int h : co.der()) {
                plafond = Math.min(plafond, h);
            }
        }
        if (co.dev() != null || co.cot() != null || co.der() != null) {
            plaque(g, face("tete2", "dessus"), volume, 12, 0.08);
            colonnes(g, "devant", co.dev(), volume, false, 8);
            colonnes(g, "gauche", co.cot(), volume, false, plafond);
            colonnes(g, "droite", co.cot(), volume, true, plafond);
            colonnes(g, "derriere", co.der(), ombre, false, 8);
        }
        for (Pose o : co.extras()) {
            new Brosse(g, "tete2", o.piece()).px(o.x(), o.y(), volume, o.largeur(), o.hauteur());
        }

        /* oreilles pointues */
        if (vrai(t.oreilles())) {
            int l = t.oreilles();
            for (String f : List.of("droite", "gauche")) {
                Brosse b = new Brosse(g, "tete2", f);
                b.px(4, 3, peau, l, 1);
                b.px(4 + l - 1, 2, peau, 1, 1);
                b.px(4, 4, shade(peau, -0.15), Math.max(1, l - 1), 1);
            }
        }

        /* oreilles animales */
        if (vrai(t.oreillesAnimales())) {
            String forme = t.oreillesAnimales();
            Brosse b = new Brosse(g, "tete2", "dessus");
            int interieur = mix(peau, 0x8A4A4A, 0.4);
            int w = 2;
            b.px(1, 3, peau, w, 2);
            b.px(8 - 1 - w, 3, peau, w, 2);
            b.px(1, 4, interieur, 1, 1);
            b.px(8 - 2, 4, interieur, 1, 1);
            Brosse bb = new Brosse(g, "tete2", "devant");
            bb.px(1, 0, peau, w, forme.equals("lupin") ? 2 : 1);
            bb.px(8 - 1 - w, 0, peau, w, forme.equals("lupin") ? 2 : 1);
        }

        /* epaulettes des tenues qui en portent */
        Tenue spec = Atlas.tenue(cfg.tenue);
        if (spec != null && spec.epaulettes()) {
            int metal = spec.ton().equals("plaque") ? 0x98A1A8 : 0x5A3B25;
            for (int i = 0; i < BRAS.size(); i++) {
                String nom = BRAS.get(i) + "2";
                for (String f : COTES) {
                    Rect r = face(nom, f);
                    plaque(g, new Rect(r.x(), r.y(), r.largeur(), 3), metal, 120 + i, 0.08);
                }
                plaque(g, face(nom, "dessus"), shade(metal, 0.08), 130 + i, 0.06);
            }
        }

        /* appendices craniens et queue : pieces de couche 2 extrudees par la figure */
        int acc = vrai(t.accent()) ? rgb(t.accent()) : shade(peau, 0.2);
        peindreAppendice(g, t.appendice(), peau, acc);
        peindreQueue(g, t.queue(), peau, acc);

        /* halo d'aasimar */
        if (vrai(t.halo())) {
            int col = rgb(vrai(t.reflet()) ? t.reflet() : "#E8C46A");
            g.save();
            g.globalAlpha = 0.55;
            Brosse b = new Brosse(g, "torse2", "derriere");
            b.px(0, 1, col, 8, 1);
            b.px(0, 2, shade(col, -0.2), 8, 1);
            g.restore();
        }
    }

    /* ---------- appendices craniens ----------
       One square of the pattern holds both sides: the figure extrudes its texels into a vertical plane laid on top
       of the skull. */

    private static void peindreAppendice(Toile g, String kind, int peau, int acc) {
        int[][][] lignes = kind == null ? null : APPENDICES.get(kind);
        if (lignes == null) {
            return;
        }
        String matiere = MAT_APPENDICE.get(kind);
        boolean oreille = matiere == null;
        int base = oreille ? peau : matiere.equals("accent") ? acc : rgb(matiere);
        int sombre = shade(base, -0.3);
        int clair = shade(base, 0.16);
        Brosse b = new Brosse(g, "appendice", "devant");
        for (int y = 0; y < lignes.length; y++) {
            for (int[] bande : lignes[y]) {
                int x = bande[0];
                int w = bande[1];
                boolean droite = x < 4;
                for (int i = 0; i < w; i++) {
                    int px = x + i;
                    int col;
                    if (!oreille) {
                        col = y < 2 ? clair : y % 2 != 0 ? shade(base, -0.18) : base;
                    } else {
                        boolean interne = droite ? px == x + w - 1 : px == x;
                        col = y == 0 ? sombre : interne && y > 1 ? acc : base;
                    }
                    b.px(px, y, col);
                }
            }
        }
    }

    private static void peindreQueue(Toile g, String kind, int peau, int acc) {
        int[][] lignes = kind == null ? null : QUEUES.get(kind);
        if (lignes == null) {
            return;
        }
        int sombre = shade(peau, -0.24);
        Brosse b = new Brosse(g, "queue", "devant");
        for (int y = 0; y < lignes.length; y++) {
            int x = lignes[y][0];
            int w = lignes[y][1];
            for (int i = 0; i < w; i++) {
                int col = peau;
                if (kind.equals("longue") && y % 2 != 0) {
                    col = sombre;
                }
                if (kind.equals("touffue") && bruit(x + i, y, 53) > 0.6) {
                    col = sombre;
                }
                if (kind.equals("panache") && y > 5) {
                    col = acc;
                }
                b.px(x + i, y, col);
            }
        }
    }

    /* Hair: two pieces each painted in its own 8x8 square of the pattern. The figure extrudes their texels;
       nothing is painted on the skin nor on the torso. */

    private static void peindreChevelure(Toile g, String coiffure, int cheveux, Traits t) {
        Coiffure co = Atlas.coiffure(coiffure);
        if (co == null || !vrai(co.chute())) {
            return;
        }
        int[][] lignes = CHUTES.get(co.chute());
        if (lignes == null) {
            return;
        }
        int col = vrai(t.chevelureEtheree()) ? shade(cheveux, 0.18) : cheveux;
        int clair = shade(col, 0.1);
        int sombre = shade(col, -0.2);
        boolean miroir = MIROIR_CHUTE.contains(co.chute());
        Brosse b = new Brosse(g, "chevelure", "devant");
        for (int y = 0; y < lignes.length; y++) {
            int[] bande = lignes[y];
            if (bande.length == 0) {
                continue;
            }
            for (int i = 0; i < bande[1]; i++) {
                poseMeche(b, bande[0] + i, y, col, clair, sombre);
                if (miroir) {
                    poseMeche(b, 8 - bande[0] - bande[1] + i, y, col, clair, sombre);
                }
            }
        }
    }

    private static void poseMeche(Brosse b, int x, int y, int col, int clair, int sombre) {
        double n = bruit(x, y, 47);
        b.px(x, y, n > 0.84 ? clair : n < 0.22 ? sombre : col);
    }

    private static void peindreBarbe(Toile g, String style, int cheveux) {
        Barbe spec = Atlas.barbe(style);
        if (spec == null || spec.rangs() == null) {
            return;
        }
        int clair = shade(cheveux, 0.1);
        int sombre = shade(cheveux, -0.22);
        Brosse b = new Brosse(g, "barbe", "devant");
        int[][][] rangs = spec.rangs();
        for (int y = 0; y < rangs.length; y++) {
            for (int[] bande : rangs[y]) {
                for (int i = 0; i < bande[1]; i++) {
                    double n = bruit(bande[0] + i, y, 31);
                    b.px(bande[0] + i, y, n > 0.84 ? clair : n < 0.2 ? sombre : cheveux);
                }
            }
        }
        if ("tressee".equals(style) || "nattee".equals(style)) {
            b.px(3, 5, sombre, 1, 3);
        }
    }
}
