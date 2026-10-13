// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

/** A painted 64x64 skin: straight (not premultiplied) RGBA, row by row from the top-left texel. */
public final class Planche {

    /** Side of the sheet, in texels. */
    public static final int TAILLE = Toile.TAILLE;

    private final byte[] rgba;

    Planche(int[] px) {
        rgba = new byte[px.length];
        for (int i = 0; i < px.length; i++) {
            rgba[i] = (byte) px[i];
        }
    }

    /** @return the texel at (x, y) packed as {@code 0xRRGGBBAA} */
    public int rgba(int x, int y) {
        int k = (y * TAILLE + x) * 4;
        return (rgba[k] & 255) << 24 | (rgba[k + 1] & 255) << 16 | (rgba[k + 2] & 255) << 8 | rgba[k + 3] & 255;
    }

    /** @return a copy of the sheet's bytes, four per texel, R G B A */
    public byte[] octets() {
        return rgba.clone();
    }
}
