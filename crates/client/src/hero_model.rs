//! Le corps du héros : les boîtes du patron 64 × 64 (`tools/heros/construire.py` de la branche
//! `develop`), aux proportions de son espèce, partagé par l'aperçu du créateur et par la vue à la
//! troisième personne, où le héros est ajouté au maillage des créatures avec sa propre planche.
//!
//! **En relief.** La maquette extrudait les pixels de la planche plutôt que de les poser à plat :
//! - la couche 2 (cheveux, capuche, tenue, halo…) n'est plus une coque lisse mais un texel = un
//!   pavé posé sur la couche 1, si bien qu'une mèche ou un col a de l'épaisseur et que ce qui est
//!   transparent ne se dessine pas du tout ;
//! - les pièces des cases libres de la planche prennent du volume : barbe et cheveux tombants,
//!   appendices crâniens (oreilles de bête, cornes, bois, couronnes, cristaux) dressés sur le
//!   crâne, queue accrochée au bas du dos et ailes spectrales déployées dans le dos ;
//! - les proportions de l'espèce et de sa variante (`mods`) : tête, torse, bras, jambes, dos voûté
//!   et lévitation du sylphe.
//!
//! **Articulé.** La figure porte 13 os (`Bone`) : corps, torse, tête, bras, avant-bras, mains,
//! jambes et tibias. Un bras ou une jambe reste un seul pavé de la planche, mais il se tord en
//! courbe au coude, au poignet et au genou : chaque coin suit l'os du haut, puis une part
//! croissante de l'os du bas à travers le pli. `FigurePose` donne la rotation et le déplacement de
//! chaque os ; `Bone::rotation_from_degrees` lit les angles des fichiers de gestes.
//!
//! Une pièce dont la case est vide (pas de barbe, pas de queue…) ne donne aucun pavé : la figure
//! se lit sur la planche peinte, et rien d'autre ne décide de ce qui se voit.

use glam::{vec3, Quat, Vec3};
use herocraft_common::hero::{atlas, paint, Appearance, Sheet, SHEET_SIZE};
use herocraft_common::tangent_frame;

use crate::creatures::CreatureFrame;
use crate::planet_mesh::PlanetVertex;

/// Indice de texture réservé à la planche du héros dans les plages du maillage des créatures
pub(crate) const HERO_TEXTURE: usize = usize::MAX;
/// Premier indice de texture des planches des habitants (`CreatureRenderer::villager_texture`)
pub(crate) const VILLAGER_TEXTURE_BASE: usize = usize::MAX / 2;

/// Un quadrilatère de la figure : coins vus de dehors (bas gauche, bas droit, haut droit, haut
/// gauche) et le rectangle de la planche qu'il montre, en texels
#[derive(Clone, Copy, Debug)]
pub(crate) struct Quad {
    pub corners: [Vec3; 4],
    pub uv: [f32; 4],
}

impl Quad {
    /// Normale sortante (non normée)
    pub(crate) fn normal(&self) -> Vec3 {
        (self.corners[1] - self.corners[0]).cross(self.corners[3] - self.corners[0])
    }

    pub(crate) fn centre(&self) -> Vec3 {
        (self.corners[0] + self.corners[1] + self.corners[2] + self.corners[3]) * 0.25
    }
}

/// Les os du héros. Chaque pièce de la figure en suit un ; un os tourne autour de son pivot et
/// emporte ses enfants (l'avant-bras suit le bras, la main suit l'avant-bras…).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Bone {
    /// Tout le héros, pivot aux pieds
    Body,
    /// Buste, tête et bras, pivot aux hanches (celui du dos voûté)
    Torso,
    /// Pivot au cou
    Head,
    /// Pivot à l'épaule
    RightArm,
    LeftArm,
    /// Pivot au coude
    RightForearm,
    LeftForearm,
    /// Pivot au poignet ; la main porte l'arme
    RightHand,
    LeftHand,
    /// Pivot à la hanche
    RightLeg,
    LeftLeg,
    /// Pivot au genou
    RightShin,
    LeftShin,
}

impl Bone {
    pub(crate) const COUNT: usize = 13;
    /// Tous les os, chaque parent avant ses enfants
    pub(crate) const ALL: [Bone; Bone::COUNT] = [
        Bone::Body,
        Bone::Torso,
        Bone::Head,
        Bone::RightArm,
        Bone::LeftArm,
        Bone::RightForearm,
        Bone::LeftForearm,
        Bone::RightHand,
        Bone::LeftHand,
        Bone::RightLeg,
        Bone::LeftLeg,
        Bone::RightShin,
        Bone::LeftShin,
    ];

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// Nom de l'os dans les fichiers de gestes
    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn name(self) -> &'static str {
        match self {
            Bone::Body => "body",
            Bone::Torso => "torso",
            Bone::Head => "head",
            Bone::RightArm => "rightArm",
            Bone::LeftArm => "leftArm",
            Bone::RightForearm => "rightForearm",
            Bone::LeftForearm => "leftForearm",
            Bone::RightHand => "rightHand",
            Bone::LeftHand => "leftHand",
            Bone::RightLeg => "rightLeg",
            Bone::LeftLeg => "leftLeg",
            Bone::RightShin => "rightShin",
            Bone::LeftShin => "leftShin",
        }
    }

    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn from_name(name: &str) -> Option<Bone> {
        Bone::ALL.into_iter().find(|b| b.name() == name)
    }

    /// L'os qui porte celui-ci (aucun pour `Body`)
    pub(crate) fn parent(self) -> Option<Bone> {
        Some(match self {
            Bone::Body => return None,
            Bone::Torso | Bone::RightLeg | Bone::LeftLeg => Bone::Body,
            Bone::Head | Bone::RightArm | Bone::LeftArm => Bone::Torso,
            Bone::RightForearm => Bone::RightArm,
            Bone::LeftForearm => Bone::LeftArm,
            Bone::RightHand => Bone::RightForearm,
            Bone::LeftHand => Bone::LeftForearm,
            Bone::RightShin => Bone::RightLeg,
            Bone::LeftShin => Bone::LeftLeg,
        })
    }

    /// Côté du héros : 1 pour la gauche (+x), −1 pour la droite, 0 au milieu
    fn side(self) -> f32 {
        match self {
            Bone::RightArm | Bone::RightForearm | Bone::RightHand | Bone::RightLeg | Bone::RightShin => -1.0,
            Bone::LeftArm | Bone::LeftForearm | Bone::LeftHand | Bone::LeftLeg | Bone::LeftShin => 1.0,
            _ => 0.0,
        }
    }

    /// Rotation d'un fichier de gestes, `[x, y, z]` en degrés, dans le repère de la figure.
    ///
    /// - x positif : bras, avant-bras, main et jambe partent vers l'avant (bras levé, coude plié
    ///   vers le haut, pointe de l'arme inclinée vers l'avant) ; torse et tête se penchent en
    ///   avant ; le tibia se plie vers l'arrière (genou fléchi) ;
    /// - y positif : l'os tourne vers la gauche du héros ;
    /// - z positif : un membre s'écarte vers l'extérieur (en miroir à gauche et à droite) ; torse et
    ///   tête penchent vers la gauche.
    ///
    /// Ordre : z, puis x, puis y (le cap en dernier).
    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn rotation_from_degrees(self, [x, y, z]: [f32; 3]) -> Quat {
        let (x, y, z) = (x.to_radians(), y.to_radians(), z.to_radians());
        // Un membre pend vers −y : l'envoyer vers +z demande une rotation x négative
        let forward = match self {
            Bone::Body | Bone::Torso | Bone::Head | Bone::RightShin | Bone::LeftShin => 1.0,
            _ => -1.0,
        };
        // Écarter vers l'extérieur : −x à droite, +x à gauche ; pencher à gauche : haut vers +x
        let outward = match self.side() {
            0.0 => -1.0,
            side => side,
        };
        Quat::from_rotation_y(y) * Quat::from_rotation_x(forward * x) * Quat::from_rotation_z(outward * z)
    }
}

/// La pose d'un os, par rapport à son parent : une rotation autour de son pivot puis un
/// déplacement, en unités du patron (`Figure::units_per_block` pour passer des blocs)
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BonePose {
    pub rotation: Quat,
    pub translation: Vec3,
}

impl Default for BonePose {
    fn default() -> Self {
        Self { rotation: Quat::IDENTITY, translation: Vec3::ZERO }
    }
}

/// Où se trouve un os posé : `p ↦ rotation · p + translation`, des unités du patron au repos vers
/// la figure posée
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BoneFrame {
    pub rotation: Quat,
    pub translation: Vec3,
}

impl BoneFrame {
    const IDENTITY: BoneFrame = BoneFrame { rotation: Quat::IDENTITY, translation: Vec3::ZERO };

    pub(crate) fn apply(&self, p: Vec3) -> Vec3 {
        self.rotation * p + self.translation
    }

    fn then(&self, local: &BoneFrame) -> BoneFrame {
        BoneFrame { rotation: self.rotation * local.rotation, translation: self.rotation * local.translation + self.translation }
    }
}

/// Une articulation le long d'un membre : l'os qui part de là, la hauteur de son pivot au repos,
/// et la demi-longueur de la courbe du pli
#[derive(Clone, Copy, Debug)]
pub(crate) struct Joint {
    pub bone: Bone,
    pub height: f32,
    pub bend: f32,
}

impl Joint {
    /// Part de la rotation de l'os que suit un point à la hauteur `y` : rien au-dessus du pli, tout
    /// en dessous, et la courbe entre les deux
    fn weight(&self, y: f32) -> f32 {
        // Une part qui croît au même pas sur tout le pli : la courbe est un arc régulier et le
        // dessus du coude s'étire partout pareil, sans qu'un rang de texels s'allonge seul
        ((self.height + self.bend - y) / (2.0 * self.bend)).clamp(0.0, 1.0)
    }
}

/// Une pièce de la figure : ses faces, l'os qui la porte et, pour un bras ou une jambe, les
/// articulations où le pavé se plie (de haut en bas). Le membre reste un seul pavé, tordu en
/// courbe au pli : la planche ne se découpe pas et aucun creux ne s'ouvre.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub quads: Vec<Quad>,
    pub bone: Bone,
    pub joints: Vec<Joint>,
}

/// Le héros en unités du patron, pieds à zéro, face vers +z, sa gauche vers +x
#[derive(Clone, Debug)]
pub(crate) struct Figure {
    pub pieces: Vec<Piece>,
    /// Pivot de chaque os au repos, dans l'ordre de `Bone::ALL`
    pivots: [Vec3; Bone::COUNT],
    /// Hauteur du corps, des pieds au sommet du crâne (sans couche 2 ni appendice)
    pub body_height: f32,
    /// Plus haut point de la figure, appendices compris
    pub top: f32,
    /// Hauteur des hanches, pivot du dos voûté
    #[allow(dead_code)]
    hips: f32,
    /// Inclinaison du haut du corps vers l'avant, en radians
    stoop: f32,
    /// Hauteur à laquelle flotte un sylphe
    pub hover: f32,
}

impl Figure {
    /// Les faces dans leur pose : membres balancés de `swing` radians, dos voûté, élargies de
    /// `width` en travers et en profondeur
    #[cfg(test)]
    pub(crate) fn posed(&self, swing: f32, width: f32) -> impl Iterator<Item = Quad> + '_ {
        self.posed_with(&FigurePose { swing, ..FigurePose::default() }, width)
    }

    /// Pivot de l'os au repos
    pub(crate) fn pivot(&self, bone: Bone) -> Vec3 {
        self.pivots[bone.index()]
    }

    /// Unités du patron par bloc pour une apparence (pour les déplacements des gestes, en blocs)
    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn units_per_block(&self, cfg: &Appearance) -> f32 {
        self.body_height / cfg.taille as f32
    }

    /// Où est chaque os dans la pose `pose` (avant élargissement et lévitation)
    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn bone_frames(&self, pose: &FigurePose) -> [BoneFrame; Bone::COUNT] {
        self.bone_frames_and_locals(pose).0
    }

    /// Les os posés, et la rotation et le déplacement de chacun par rapport à son parent
    fn bone_frames_and_locals(&self, pose: &FigurePose) -> ([BoneFrame; Bone::COUNT], [(Quat, Vec3); Bone::COUNT]) {
        let mut frames = [BoneFrame::IDENTITY; Bone::COUNT];
        let mut locals = [(Quat::IDENTITY, Vec3::ZERO); Bone::COUNT];
        for bone in Bone::ALL {
            let mut rotation = pose.bones[bone.index()].rotation;
            // La marche balance bras et jambes, en sens contraires ; le dos voûté penche le torse
            let swing = match bone {
                Bone::RightArm | Bone::LeftLeg => 1.0,
                Bone::LeftArm | Bone::RightLeg => -1.0,
                _ => 0.0,
            };
            if swing != 0.0 {
                rotation *= Quat::from_rotation_x(swing * pose.swing);
            }
            if bone == Bone::Torso {
                rotation *= Quat::from_rotation_x(self.stoop);
            }
            let pivot = self.pivot(bone);
            let translation = pose.bones[bone.index()].translation;
            locals[bone.index()] = (rotation, translation);
            let local = BoneFrame { rotation, translation: pivot + translation - rotation * pivot };
            frames[bone.index()] = match bone.parent() {
                Some(parent) => frames[parent.index()].then(&local),
                None => local,
            };
        }
        (frames, locals)
    }

    /// Les faces dans une pose complète : comme `posed`, chaque os tourné et déplacé en plus
    pub(crate) fn posed_with(&self, pose: &FigurePose, width: f32) -> impl Iterator<Item = Quad> + use<'_> {
        self.posed_bones(pose, width).map(|(_, quad)| quad)
    }

    /// Comme `posed_with`, avec l'os de la pièce qui porte chaque quadrilatère
    pub(crate) fn posed_bones(&self, pose: &FigurePose, width: f32) -> impl Iterator<Item = (Bone, Quad)> + use<'_> {
        let scale = vec3(width, 1.0, width);
        let lift = vec3(0.0, self.hover, 0.0);
        let (frames, locals) = self.bone_frames_and_locals(pose);
        self.pieces.iter().flat_map(move |piece| {
            let frame = frames[piece.bone.index()];
            piece.quads.iter().map(move |q| {
                let corners = q.corners.map(|p| {
                    // Chaque coin suit l'os du membre, puis une part de chaque articulation selon
                    // sa hauteur au repos : le pavé se courbe au pli
                    let mut f = frame;
                    for joint in &piece.joints {
                        let w = joint.weight(p.y);
                        if w <= 0.0 {
                            break;
                        }
                        let local = locals[joint.bone.index()];
                        let pivot = self.pivot(joint.bone);
                        let rotation = Quat::IDENTITY.slerp(local.0, w);
                        let partial = BoneFrame { rotation, translation: pivot + local.1 * w - rotation * pivot };
                        f = f.then(&partial);
                    }
                    f.apply(p) * scale + lift
                });
                (piece.bone, Quad { corners, uv: q.uv })
            })
        })
    }

    /// Nombre de faces
    #[cfg(test)]
    pub(crate) fn quad_count(&self) -> usize {
        self.pieces.iter().map(|p| p.quads.len()).sum()
    }
}

/* -------------------------------------------------------------------- boîtes et pavés */

/// Les six faces d'une boîte dépliée à la manière de Minecraft depuis son coin du patron `(u, v)`.
/// `size` : dimensions de la boîte, `pat` : celles de son dépliage en texels (elles diffèrent
/// quand l'espèce étire un membre), `g` : gonflement de chaque côté.
fn boite(cx: f32, bottom: f32, size: Vec3, pat: [f32; 3], u: f32, v: f32, g: f32) -> [Quad; 6] {
    let (x0, x1) = (cx - size.x / 2.0 - g, cx + size.x / 2.0 + g);
    let (y0, y1) = (bottom - g, bottom + size.y + g);
    let (z0, z1) = (-size.z / 2.0 - g, size.z / 2.0 + g);
    let [w, h, d] = pat;
    [
        Quad { corners: [vec3(x0, y0, z1), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x0, y1, z1)], uv: [u + d, v + d, w, h] },
        Quad { corners: [vec3(x1, y0, z0), vec3(x0, y0, z0), vec3(x0, y1, z0), vec3(x1, y1, z0)], uv: [u + d + w + d, v + d, w, h] },
        Quad { corners: [vec3(x1, y0, z1), vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x1, y1, z1)], uv: [u + d + w, v + d, d, h] },
        Quad { corners: [vec3(x0, y0, z0), vec3(x0, y0, z1), vec3(x0, y1, z1), vec3(x0, y1, z0)], uv: [u, v + d, d, h] },
        Quad { corners: [vec3(x0, y1, z1), vec3(x1, y1, z1), vec3(x1, y1, z0), vec3(x0, y1, z0)], uv: [u + d, v, w, d] },
        Quad { corners: [vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y0, z1), vec3(x0, y0, z1)], uv: [u + d + w, v, w, d] },
    ]
}

/// Les faces d'une boîte, les quatre côtés découpés en rangs d'un texel : un membre peut ainsi se
/// courber à ses articulations au lieu de rester rigide
fn en_rangs(out: &mut Vec<Quad>, faces: [Quad; 6]) {
    for (index, face) in faces.into_iter().enumerate() {
        let [u, v, w, h] = face.uv;
        let rows = h.round() as usize;
        if index >= 4 || rows <= 1 {
            out.push(face);
            continue;
        }
        let [c0, c1, c2, c3] = face.corners;
        for j in 0..rows {
            // Rang j compté du haut de la face, sur la planche comme sur la boîte
            let (top, bottom) = (1.0 - j as f32 / rows as f32, 1.0 - (j + 1) as f32 / rows as f32);
            let corners = [c0.lerp(c3, bottom), c1.lerp(c2, bottom), c1.lerp(c2, top), c0.lerp(c3, top)];
            out.push(Quad { corners, uv: [u, v + j as f32, w, 1.0] });
        }
    }
}

/// Un pavé de coin `o` et d'arêtes `a`, `b`, `c`, chaque face tournée vers le dehors et montrant
/// le texel `uv`. `show` : faces −a, +a, −b, +b, −c, +c à dessiner.
fn pave(out: &mut Vec<Quad>, o: Vec3, a: Vec3, b: Vec3, c: Vec3, uv: [f32; 4], show: [bool; 6]) {
    let centre = o + (a + b + c) * 0.5;
    let faces = [
        [o, o + b, o + b + c, o + c],
        [o + a, o + a + b, o + a + b + c, o + a + c],
        [o, o + a, o + a + c, o + c],
        [o + b, o + a + b, o + a + b + c, o + b + c],
        [o, o + a, o + a + b, o + b],
        [o + c, o + a + c, o + a + b + c, o + b + c],
    ];
    for (mut corners, shown) in faces.into_iter().zip(show) {
        if !shown {
            continue;
        }
        let quad = Quad { corners, uv };
        let middle = quad.centre();
        if quad.normal().dot(middle - centre) < 0.0 {
            corners.reverse();
        }
        out.push(Quad { corners, uv });
    }
}

/// Le texel `(x, y)` de la planche est-il peint ?
fn peint(sheet: &Sheet, x: i32, y: i32) -> bool {
    let n = SHEET_SIZE as i32;
    (0..n).contains(&x) && (0..n).contains(&y) && sheet.texel(x as usize, y as usize)[3] > 0
}

/// La couche 2 d'une boîte en relief : chaque texel peint de ses faces devient un pavé de
/// profondeur `t` posé sur la couche 1. Les faces sont prises sur la boîte gonflée de `t`, si
/// bien que les pavés de deux faces voisines se rejoignent sur l'arête.
///
/// `taper` : sur les quatre côtés, les deux derniers rangs avant le bas d'une plage peinte
/// s'amincissent. Une chevelure finit ainsi en lisière, plaquée sur la nuque et les tempes, au
/// lieu d'une marche nette qui la ferait paraître coupée au couteau.
fn relief(out: &mut Vec<Quad>, sheet: &Sheet, cx: f32, bottom: f32, size: Vec3, pat: [f32; 3], u: f32, v: f32, t: f32, taper: bool) {
    for (index, face) in boite(cx, bottom, size, pat, u, v, t).into_iter().enumerate() {
        let [fu, fv, fw, fh] = face.uv.map(|x| x as i32);
        let [c0, c1, _, c3] = face.corners;
        let (sx, sy) = ((c1 - c0) / fw as f32, (c3 - c0) / fh as f32);
        let normal = face.normal().normalize_or_zero();
        let on = |i: i32, j: i32| (0..fw).contains(&i) && (0..fh).contains(&j) && peint(sheet, fu + i, fv + j);
        // Épaisseur d'un texel, nulle s'il n'est pas peint
        let thickness = |i: i32, j: i32| -> f32 {
            if !on(i, j) {
                return 0.0;
            }
            if !taper || index >= 4 {
                return t;
            }
            let below = (1..=2).take_while(|d| on(i, j + d)).count();
            t * [0.35, 0.7, 1.0][below]
        };
        for j in 0..fh {
            for i in 0..fw {
                let d = thickness(i, j);
                if d <= 0.0 {
                    continue;
                }
                // Rang j compté du haut de la face, la face montée du bas ; le pavé part de la
                // couche 1 et s'arrête à `d` au-dessus
                let depth = normal * d;
                let o = c0 + sx * i as f32 + sy * (fh - 1 - j) as f32 - normal * t;
                // Un côté se voit s'il borde un texel vide ou plus mince (pas au bord de la face,
                // que le pavé de la face voisine couvre)
                let open = |ni: i32, nj: i32| (0..fw).contains(&ni) && (0..fh).contains(&nj) && thickness(ni, nj) < d;
                let show = [open(i - 1, j), open(i + 1, j), open(i, j + 1), open(i, j - 1), false, true];
                pave(out, o, sx, sy, depth, [(fu + i) as f32, (fv + j) as f32, 1.0, 1.0], show);
            }
        }
    }
}

/// Une pièce plate extrudée : chaque texel peint du rectangle `(u, v, w, h)` devient un pavé.
/// `origin` : coin du texel (0, 0) côté haut gauche, `step_x` / `step_y` : pas d'une colonne et
/// d'un rang (rangs descendants), `thick` : épaisseur, centrée sur le plan.
fn extrude(out: &mut Vec<Quad>, sheet: &Sheet, (u, v, w, h): (i32, i32, i32, i32), origin: Vec3, step_x: Vec3, step_y: Vec3, thick: Vec3, mirror: bool) {
    let col = |i: i32| if mirror { w - 1 - i } else { i };
    let on = |i: i32, j: i32| (0..w).contains(&i) && (0..h).contains(&j) && peint(sheet, u + col(i), v + j);
    for j in 0..h {
        for i in 0..w {
            if !on(i, j) {
                continue;
            }
            let o = origin + step_x * i as f32 + step_y * (j + 1) as f32 - thick * 0.5;
            let show = [!on(i - 1, j), !on(i + 1, j), !on(i, j + 1), !on(i, j - 1), true, true];
            pave(out, o, step_x, -step_y, thick, [(u + col(i)) as f32, (v + j) as f32, 1.0, 1.0], show);
        }
    }
}

/* -------------------------------------------------------------------- la figure */

/// Épaisseur du relief de la couche 2 : sur la tête, sur le reste du corps
const RELIEF_TETE: f32 = 0.75;
const RELIEF_CORPS: f32 = 0.5;
/// Hauteur à laquelle flotte un sylphe, en texels
const HOVER: f32 = 2.0;

/// Le corps du héros pour l'apparence `cfg`, en relief sur sa planche peinte `sheet`
pub(crate) fn figure(cfg: &Appearance, sheet: &Sheet) -> Figure {
    let a = atlas();
    let m = a.mods_de(cfg);
    let ratio = |v: Option<f64>| v.filter(|v| *v > 0.0).unwrap_or(1.0) as f32;
    let (k, sx, ab, lj) = (ratio(m.tete), ratio(m.torse), ratio(m.bras), ratio(m.jambes));
    let legs = 12.0 * lj;
    let shoulders = legs + 12.0;
    let crown = shoulders + 8.0 * k;
    let arm_x = 4.0 * sx + 2.0 * ab;

    let mut pieces = Vec::new();
    let mut piece = |bone: Bone, joints: Vec<Joint>, build: &dyn Fn(&mut Vec<Quad>)| {
        let mut quads = Vec::new();
        build(&mut quads);
        pieces.push(Piece { quads, bone, joints });
    };

    // Les plis : coude et poignet à 5 et 9 texels du haut du bras, genou à mi-jambe
    let (elbow, wrist, knee) = (legs + 7.0, legs + 3.0, legs / 2.0);
    let arm = |forearm, hand| vec![Joint { bone: forearm, height: elbow, bend: 2.0 * ab }, Joint { bone: hand, height: wrist, bend: 1.0 }];
    let leg = |shin| vec![Joint { bone: shin, height: knee, bend: 2.0 * lj }];

    // Chaque membre : sa boîte de couche 1, puis sa couche 2 en relief. Un bras ou une jambe reste
    // un seul pavé, découpé en rangs de texels pour pouvoir se courber à ses articulations.
    // (os, articulations, centre x, bas, taille, dépliage, coin couche 1, coin couche 2, relief)
    #[allow(clippy::type_complexity)]
    let membres: [(Bone, Vec<Joint>, f32, f32, Vec3, [f32; 3], (f32, f32), (f32, f32), f32); 6] = [
        (Bone::Head, vec![], 0.0, shoulders, Vec3::splat(8.0 * k), [8.0, 8.0, 8.0], (0.0, 0.0), (32.0, 0.0), RELIEF_TETE),
        (Bone::Torso, vec![], 0.0, legs, vec3(8.0 * sx, 12.0, 4.0 * sx), [8.0, 12.0, 4.0], (16.0, 16.0), (16.0, 32.0), RELIEF_CORPS),
        (Bone::RightArm, arm(Bone::RightForearm, Bone::RightHand), -arm_x, legs, vec3(4.0 * ab, 12.0, 4.0 * ab), [4.0, 12.0, 4.0], (40.0, 16.0), (40.0, 32.0), RELIEF_CORPS),
        (Bone::LeftArm, arm(Bone::LeftForearm, Bone::LeftHand), arm_x, legs, vec3(4.0 * ab, 12.0, 4.0 * ab), [4.0, 12.0, 4.0], (32.0, 48.0), (48.0, 48.0), RELIEF_CORPS),
        (Bone::RightLeg, leg(Bone::RightShin), -2.0, 0.0, vec3(4.0, legs, 4.0), [4.0, 12.0, 4.0], (0.0, 16.0), (0.0, 32.0), RELIEF_CORPS),
        (Bone::LeftLeg, leg(Bone::LeftShin), 2.0, 0.0, vec3(4.0, legs, 4.0), [4.0, 12.0, 4.0], (16.0, 48.0), (0.0, 48.0), RELIEF_CORPS),
    ];
    for (bone, joints, cx, bottom, size, pat, (u1, v1), (u2, v2), t) in membres {
        let bends = !joints.is_empty();
        piece(bone, joints, &|q| {
            let faces = boite(cx, bottom, size, pat, u1, v1, 0.0);
            if bends {
                en_rangs(q, faces);
            } else {
                q.extend(faces);
            }
            relief(q, sheet, cx, bottom, size, pat, u2, v2, t, t == RELIEF_TETE);
        });
    }

    let mouth = shoulders + 4.0 * k;
    let front = 4.0 * k + 0.05;
    let back = 4.0 * sx;
    piece(Bone::Head, vec![], &|q| {
        // Barbe : couvre le bas du visage et tombe sous le menton
        extrude(q, sheet, (0, 0, 8, 8), vec3(-4.0 * k, mouth, front + 0.5), Vec3::X * k, Vec3::NEG_Y * k, Vec3::Z, false);
        // Cheveux tombants : derrière la tête, vus de dos
        extrude(q, sheet, (24, 0, 8, 8), vec3(4.0 * k, mouth, -front - 0.5), Vec3::NEG_X * k, Vec3::NEG_Y * k, Vec3::Z, false);
        // Appendice crânien : plan dressé sur le crâne, les deux côtés dans la même case ; son
        // rang peint le plus bas s'enfonce un peu dans la chevelure
        let empty = (0..8).rev().take_while(|&j| (0..8).all(|i| !peint(sheet, 32 + i, j))).count();
        let base = crown + 0.25 - empty as f32 * k;
        extrude(q, sheet, (32, 0, 8, 8), vec3(-4.0 * k, base + 8.0 * k, 0.0), Vec3::X * k, Vec3::NEG_Y * k, Vec3::Z * k, false);
    });
    piece(Bone::Torso, vec![], &|q| {
        // Ailes : racine contre le dos, déployées vers l'arrière et les côtés
        let span = 1.3_f32;
        let sweep = 30.0_f32.to_radians();
        for side in [1.0_f32, -1.0] {
            let out = vec3(side * sweep.cos(), 0.0, -sweep.sin()) * span;
            let origin = vec3(side * 1.0, shoulders + 3.0, -back / 2.0 - 0.8);
            let thick = out.cross(Vec3::Y).normalize() * 0.4;
            extrude(q, sheet, (56, 16, 7, 14), origin, out, Vec3::NEG_Y * span, thick, false);
        }
        // Queue : accrochée au bas du dos, elle part vers l'arrière en tombant
        let down = vec3(0.0, -0.55, -0.83).normalize();
        let root = vec3(4.0, legs + 1.5, -back / 2.0 + 0.3);
        let thick = Vec3::NEG_X.cross(down).normalize();
        extrude(q, sheet, (56, 0, 8, 8), root, Vec3::NEG_X, down, thick, false);
    });

    let top = pieces.iter().flat_map(|p| p.quads.iter()).flat_map(|q| q.corners).fold(crown, |t, c| t.max(c.y));
    let pivots = Bone::ALL.map(|bone| {
        let x = bone.side() * arm_x;
        match bone {
            Bone::Body => Vec3::ZERO,
            Bone::Torso => vec3(0.0, legs, 0.0),
            Bone::Head => vec3(0.0, shoulders, 0.0),
            Bone::RightArm | Bone::LeftArm => vec3(x, legs + 10.0, 0.0),
            Bone::RightForearm | Bone::LeftForearm => vec3(x, elbow, 0.0),
            Bone::RightHand | Bone::LeftHand => vec3(x, wrist, 0.0),
            Bone::RightLeg | Bone::LeftLeg => vec3(bone.side() * 2.0, legs, 0.0),
            Bone::RightShin | Bone::LeftShin => vec3(bone.side() * 2.0, knee, 0.0),
        }
    });
    Figure {
        pieces,
        pivots,
        body_height: crown,
        top,
        hips: legs,
        stoop: (m.voute.unwrap_or(0) as f32).to_radians(),
        hover: if m.flotte == Some(true) { HOVER } else { 0.0 },
    }
}

/// Une figure et sa planche, gardées tant que l'apparence ne change pas ce qui s'y voit
#[derive(Default)]
pub(crate) struct FigureCache {
    entry: Option<(String, Figure)>,
}

impl FigureCache {
    pub(crate) fn get(&mut self, cfg: &Appearance) -> &Figure {
        let key = cfg.sheet_key();
        if self.entry.as_ref().is_none_or(|(k, _)| *k != key) {
            let sheet = paint(cfg);
            self.entry = Some((key, figure(cfg, &sheet)));
        }
        &self.entry.as_ref().expect("figure posée juste au-dessus").1
    }
}

/* -------------------------------------------------------------------- la marche */

/// Ampleur du balancement des membres à pleine allure, en radians
const SWING_MAX: f32 = 0.7;
/// Allure (blocs par seconde) à laquelle le balancement atteint son ampleur
const FULL_STRIDE_SPEED: f32 = 2.5;
/// Longueur d'un cycle de marche (un pas de chaque pied) à l'arrêt, en blocs, et ce qu'elle gagne
/// par bloc par seconde d'allure : on allonge le pas en pressant l'allure. Un habitant qui marche à
/// 1,6 bloc par seconde fait ainsi près d'un cycle par seconde (deux pas), comme un marcheur.
const STRIDE_BASE_BLOCKS: f32 = 1.1;
const STRIDE_PER_SPEED: f32 = 0.3;

/// La marche du héros : le balancement de ses membres suit son déplacement
#[derive(Default)]
pub(crate) struct HeroGait {
    last_feet: Option<Vec3>,
    phase: f32,
    amplitude: f32,
    /// Allure lissée, en blocs par seconde (les positions reçues du réseau arrivent par à-coups)
    speed: f32,
}

impl HeroGait {
    /// Avance la marche de `dt` secondes vers la position `feet` ; `block` : taille d'un bloc
    pub(crate) fn update(&mut self, feet: Vec3, dt: f32, block: f32) {
        let last = self.last_feet.replace(feet).unwrap_or(feet);
        if dt <= 0.0 {
            return;
        }
        let up = feet.normalize_or_zero();
        let step = feet - last;
        // Seul le pas sur le sol compte ; un saut d'un monde à l'autre ne fait pas courir
        let ground = (step - up * step.dot(up)).length() / block;
        let (ground, speed) = if ground > 50.0 { (0.0, 0.0) } else { (ground, ground / dt) };
        self.speed += (speed - self.speed) * (dt * 8.0).min(1.0);
        // Le balancement avance d'un cycle à chaque longueur de pas parcourue
        let stride = STRIDE_BASE_BLOCKS + STRIDE_PER_SPEED * self.speed;
        self.phase = (self.phase + ground * std::f32::consts::TAU / stride) % std::f32::consts::TAU;
        let target = (self.speed / FULL_STRIDE_SPEED).min(1.0) * SWING_MAX;
        // Les bras retombent en douceur à l'arrêt
        self.amplitude += (target - self.amplitude) * (dt * 10.0).min(1.0);
    }

    /// Angle de balancement du moment
    pub(crate) fn swing(&self) -> f32 {
        self.amplitude * self.phase.sin()
    }
}

/// Ajoute le héros au maillage des créatures, debout sur `feet` et tourné vers `yaw` (0 = nord),
/// dans la pose `pose` (balancement de la marche, geste d'une compétence) ; `block` : taille d'un bloc
pub(crate) fn push_hero_posed(frame: &mut CreatureFrame, cfg: &Appearance, figure: &Figure, feet: Vec3, yaw: f32, pose: &FigurePose, block: f32) {
    push_figure(frame, cfg, figure, feet, yaw, pose, HERO_TEXTURE, block);
}

/// Os des bras, seuls dessinés à la première personne
const ARM_BONES: [Bone; 6] = [Bone::RightArm, Bone::LeftArm, Bone::RightForearm, Bone::LeftForearm, Bone::RightHand, Bone::LeftHand];

/// Vue à la première personne, pendant le geste d'une compétence : les bras du héros posé (et
/// l'arme de sa main droite, s'il en tient une), accrochés à l'œil `eye` qui regarde vers `look`.
/// Renvoie les points des mains, où se jouent les effets.
pub(crate) fn push_first_person_arms(frame: &mut CreatureFrame, cfg: &Appearance, figure: &Figure, eye: Vec3, look: Vec3, pose: &FigurePose, weapon: Option<[f32; 4]>, block: f32) -> (Vec3, Vec3) {
    let up_hint = eye.normalize_or_zero();
    let forward = look.normalize_or(up_hint.any_orthonormal_vector());
    let left = up_hint.cross(forward).normalize_or(forward.any_orthonormal_vector());
    let up = forward.cross(left);
    let unit = cfg.taille as f32 / figure.body_height * block;
    // Le cou du héros juste sous l'œil, le corps un peu en retrait : les épaules sortent du champ
    // au repos, les bras y entrent quand le geste les lève devant soi
    let neck = figure.pivot(Bone::Head) * vec3(cfg.width_scale(), 1.0, cfg.width_scale());
    let origin = eye - (left * neck.x + up * neck.y + forward * neck.z) * unit - up * 0.12 * block + forward * 0.1 * block;
    let to_world = |p: Vec3| origin + (left * p.x + up * p.y + forward * p.z) * unit;
    let rotate = |n: Vec3| left * n.x + up * n.y + forward * n.z;
    let sheet = SHEET_SIZE as f32;
    let margin = 0.01_f32;
    let color = [pose.tint[0], pose.tint[1], pose.tint[2], 1.0];
    let start = frame.indices.len() as u32;
    for (bone, quad) in figure.posed_bones(pose, cfg.width_scale()) {
        if !ARM_BONES.contains(&bone) {
            continue;
        }
        let c = quad.corners;
        let normal = rotate((c[1] - c[0]).cross(c[3] - c[0]).normalize_or_zero());
        let [u, v, w, h] = quad.uv;
        let (u0, v0, u1, v1) = ((u + margin) / sheet, (v + margin) / sheet, (u + w - margin) / sheet, (v + h - margin) / sheet);
        let uvs = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
        let base = frame.vertices.len() as u32;
        for (p, uv) in c.iter().zip(uvs) {
            frame.vertices.push(PlanetVertex { position: to_world(*p).to_array(), normal: normal.to_array(), color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });
            frame.uvs.push(uv);
        }
        frame.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    frame.draws.push((Some(HERO_TEXTURE), start..frame.indices.len() as u32));

    let frames = figure.bone_frames(pose);
    let at = |bone: Bone| to_world(frames[bone.index()].apply(figure.pivot(bone)));
    let (right_hand, left_hand) = (at(Bone::RightHand), at(Bone::LeftHand));
    if let Some(color) = weapon {
        // Une arme simple dans le poing : manche de bois, puis la lame ou la tête aux couleurs de
        // l'objet, dans l'axe du poing (devant soi quand le bras pend)
        let grip = right_hand + rotate(frames[Bone::RightHand.index()].rotation * Vec3::NEG_Y) * 1.5 * unit;
        let along = rotate(frames[Bone::RightHand.index()].rotation * Vec3::Z).normalize_or(forward);
        let side = rotate(frames[Bone::RightHand.index()].rotation * Vec3::X).normalize_or(left);
        let start = frame.indices.len() as u32;
        push_box(frame, grip - along * 0.08 * block, along, side, 0.3 * block, 0.05 * block, [0.36, 0.24, 0.14, 1.0]);
        push_box(frame, grip + along * 0.22 * block, along, side, 0.55 * block, 0.07 * block, color);
        frame.draws.push((None, start..frame.indices.len() as u32));
    }
    (right_hand, left_hand)
}

/// Pavé de couleur unie : de `base` sur `length` le long de `along`, de côté `width`
fn push_box(frame: &mut CreatureFrame, base: Vec3, along: Vec3, side: Vec3, length: f32, width: f32, color: [f32; 4]) {
    let third = along.cross(side).normalize_or_zero();
    let h = width * 0.5;
    let corner = |a: f32, s: f32, t: f32| base + along * a + side * s + third * t;
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[0.0, -1.0, 1.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, -1.0, 1.0]]),
        ([0.0, 0.0, -1.0], [[0.0, 1.0, -1.0], [0.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0]]),
        ([0.0, 1.0, 0.0], [[0.0, 1.0, 1.0], [0.0, 1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0]]),
        ([0.0, -1.0, 0.0], [[0.0, -1.0, -1.0], [0.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, -1.0, -1.0]]),
        ([1.0, 0.0, 0.0], [[1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0], [1.0, -1.0, -1.0]]),
        ([-1.0, 0.0, 0.0], [[0.0, -1.0, -1.0], [0.0, 1.0, -1.0], [0.0, 1.0, 1.0], [0.0, -1.0, 1.0]]),
    ];
    for (n, corners) in faces {
        let normal = side * n[1] + third * n[2] + along * n[0];
        let base_index = frame.vertices.len() as u32;
        for [a, s, t] in corners {
            frame.vertices.push(PlanetVertex { position: corner(a * length, s * h, t * h).to_array(), normal: normal.to_array(), color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });
            frame.uvs.push([0.5, 0.5]);
        }
        frame.indices.extend_from_slice(&[base_index, base_index + 1, base_index + 2, base_index, base_index + 2, base_index + 3]);
    }
}

/// Points du héros posé où se jouent les effets des compétences, dans le monde
#[derive(Debug, Clone, Copy)]
pub(crate) struct HeroAnchors {
    pub feet: Vec3,
    pub body: Vec3,
    pub head: Vec3,
    pub right_hand: Vec3,
    pub left_hand: Vec3,
}

impl HeroAnchors {
    /// Points approchés d'un héros qu'on ne dessine pas (vue à la première personne, autre joueur)
    pub(crate) fn standing(feet: Vec3, yaw: f32, block: f32) -> Self {
        let up = feet.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let forward = north * yaw.cos() + east * yaw.sin();
        let left = up.cross(forward);
        let body = feet + up * 1.0 * block;
        Self { feet, body, head: feet + up * 1.6 * block, right_hand: body - left * 0.35 * block + forward * 0.35 * block, left_hand: body + left * 0.35 * block + forward * 0.35 * block }
    }

    /// Points du héros `figure` posé en `pose`, debout sur `feet` et tourné vers `yaw`
    pub(crate) fn posed(cfg: &Appearance, figure: &Figure, feet: Vec3, yaw: f32, pose: &FigurePose, block: f32) -> Self {
        let up = feet.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let forward = north * yaw.cos() + east * yaw.sin();
        let left = up.cross(forward);
        let unit = cfg.taille as f32 / figure.body_height * block;
        let to_world = |p: Vec3| feet + (left * p.x + up * p.y + forward * p.z) * unit;
        let frames = figure.bone_frames(pose);
        let at = |bone: Bone| to_world(frames[bone.index()].apply(figure.pivot(bone)));
        Self { feet, body: at(Bone::Torso), head: at(Bone::Head), right_hand: at(Bone::RightHand), left_hand: at(Bone::LeftHand) }
    }
}

/// Posture d'une figure : balancement de la marche, pose de chaque os, couchée, teinte
#[derive(Debug, Clone, Copy)]
pub(crate) struct FigurePose {
    /// Balancement des bras et des jambes
    pub swing: f32,
    /// Rotation et déplacement de chaque os par rapport à son parent, dans l'ordre de `Bone::ALL`,
    /// par-dessus le balancement. Un os laissé au repos reste dans le prolongement de son parent.
    pub bones: [BonePose; Bone::COUNT],
    /// De 0 (debout) à 1 (couchée sur le dos)
    pub lying: f32,
    /// Couleur multipliée à la planche (rouge d'un coup reçu)
    pub tint: [f32; 3],
}

impl Default for FigurePose {
    fn default() -> Self {
        Self { swing: 0.0, bones: [BonePose::default(); Bone::COUNT], lying: 0.0, tint: [1.0; 3] }
    }
}

impl FigurePose {
    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn bone(&self, bone: Bone) -> &BonePose {
        &self.bones[bone.index()]
    }

    pub(crate) fn bone_mut(&mut self, bone: Bone) -> &mut BonePose {
        &mut self.bones[bone.index()]
    }

    /// Raccourci des habitants : lève le bras droit vers l'avant de `angle` radians
    pub(crate) fn raise_right_arm(&mut self, angle: f32) {
        self.bone_mut(Bone::RightArm).rotation = Quat::from_rotation_x(-angle);
    }

    /// Raccourci des habitants : lève le bras gauche vers l'avant de `angle` radians
    pub(crate) fn raise_left_arm(&mut self, angle: f32) {
        self.bone_mut(Bone::LeftArm).rotation = Quat::from_rotation_x(-angle);
    }

    /// De combien le bras `bone` est levé vers l'avant, en radians
    #[allow(dead_code)] // lu par les gestes des compétences
    pub(crate) fn arm_raise(&self, bone: Bone) -> f32 {
        let down = self.bone(bone).rotation * Vec3::NEG_Y;
        down.z.atan2(-down.y)
    }
}

/// Ajoute une figure humanoïde au maillage des créatures avec la planche d'indice `texture`
pub(crate) fn push_figure(frame: &mut CreatureFrame, cfg: &Appearance, figure: &Figure, feet: Vec3, yaw: f32, pose: &FigurePose, texture: usize, block: f32) {
    let up = feet.normalize_or_zero();
    let (east, north) = tangent_frame(up);
    let forward = north * yaw.cos() + east * yaw.sin();
    let left = up.cross(forward);
    // La taille du héros va des pieds au sommet du crâne, quelles que soient ses proportions
    let unit = cfg.taille as f32 / figure.body_height * block;
    // Couchée : la figure bascule en arrière autour de ses pieds, soulevée de l'épaisseur du corps
    let lying = pose.lying.clamp(0.0, 1.0);
    let lie = Quat::from_rotation_x(-lying * std::f32::consts::FRAC_PI_2);
    let lift = vec3(0.0, 2.0 * lying, 0.0);
    let to_world = |p: Vec3| feet + (left * p.x + up * p.y + forward * p.z) * unit;
    let rotate = |n: Vec3| left * n.x + up * n.y + forward * n.z;
    let sheet = SHEET_SIZE as f32;
    let margin = 0.01_f32;
    let color = [pose.tint[0], pose.tint[1], pose.tint[2], 1.0];

    let start = frame.indices.len() as u32;
    for quad in figure.posed_with(pose, cfg.width_scale()) {
        let c = quad.corners.map(|p| lie * p + lift);
        let normal = rotate((c[1] - c[0]).cross(c[3] - c[0]).normalize_or_zero());
        let [u, v, w, h] = quad.uv;
        let (u0, v0, u1, v1) = ((u + margin) / sheet, (v + margin) / sheet, (u + w - margin) / sheet, (v + h - margin) / sheet);
        let uvs = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
        let base = frame.vertices.len() as u32;
        for (p, uv) in c.iter().zip(uvs) {
            frame.vertices.push(PlanetVertex { position: to_world(*p).to_array(), normal: normal.to_array(), color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });
            frame.uvs.push(uv);
        }
        frame.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    frame.draws.push((Some(texture), start..frame.indices.len() as u32));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(espece: &str, variante: i32) -> (Appearance, Figure) {
        let mut cfg = atlas().defaut(espece);
        cfg.variante = variante;
        cfg.retouch(true);
        let f = figure(&cfg, &paint(&cfg));
        (cfg, f)
    }

    #[test]
    fn first_person_arms_come_into_view_during_a_slam() {
        let (cfg, f) = hero("humain", 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/hero/animations");
        let (gestures, _) = crate::gestures::GestureLibrary::load_dir(&dir);
        let slam = gestures.get("groundSlam").unwrap();
        let eye = Vec3::new(0.0, 1000.0, 0.0);
        let look = Vec3::new(0.0, -0.2, -1.0).normalize();
        let in_view = |p: Vec3| (p - eye).normalize().dot(look) > 0.6;
        let mut frame = CreatureFrame::default();
        let mut pose = FigurePose::default();
        slam.pose(0.0, f.units_per_block(&cfg), &mut pose);
        let (rest, _) = push_first_person_arms(&mut frame, &cfg, &f, eye, look, &pose, Some([0.7; 4]), 1.0);
        assert!(!in_view(rest), "au repos, la main pend hors du champ");
        assert!(!frame.indices.is_empty() && frame.draws.len() == 2, "bras et arme");
        let mut seen = false;
        for i in 0..=20 {
            slam.pose(i as f32 / 20.0, f.units_per_block(&cfg), &mut pose);
            let (hand, _) = push_first_person_arms(&mut CreatureFrame::default(), &cfg, &f, eye, look, &pose, None, 1.0);
            seen |= in_view(hand);
            assert!(hand.distance(eye) < 1.5, "la main reste près de l'œil : {}", hand.distance(eye));
        }
        assert!(seen, "pendant le geste, la main passe devant l'œil");
    }

    fn points(f: &Figure) -> Vec<Vec3> {
        f.posed(0.0, 1.0).flat_map(|q| q.corners).collect()
    }

    /// Les coins posés de l'os `bone` le long d'un bras ou d'une jambe : ceux de sa pièce
    fn limb(f: &Figure, pose: &FigurePose, bone: Bone) -> Vec<Vec3> {
        let start: usize = f.pieces.iter().take_while(|p| p.bone != bone).map(|p| p.quads.len()).sum();
        let count = f.pieces.iter().find(|p| p.bone == bone).unwrap().quads.len();
        f.posed_with(pose, 1.0).skip(start).take(count).flat_map(|q| q.corners).collect()
    }

    #[test]
    fn bones_have_english_names_and_parents_come_first() {
        for (i, bone) in Bone::ALL.into_iter().enumerate() {
            assert_eq!(bone.index(), i);
            assert_eq!(Bone::from_name(bone.name()), Some(bone));
            if let Some(parent) = bone.parent() {
                assert!(parent.index() < i, "{} avant {}", parent.name(), bone.name());
            }
        }
        assert_eq!(Bone::from_name("rightForearm"), Some(Bone::RightForearm));
        assert_eq!(Bone::from_name("avantBras"), None);
    }

    #[test]
    fn gesture_angles_follow_the_files_conventions() {
        let dir = |bone: Bone, r: [f32; 3]| bone.rotation_from_degrees(r) * Vec3::NEG_Y;
        // Bras tendu devant soi, ouvert sur le côté, en miroir
        assert!(dir(Bone::RightArm, [90.0, 0.0, 0.0]).abs_diff_eq(Vec3::Z, 1e-5));
        assert!(dir(Bone::RightArm, [0.0, 0.0, 90.0]).abs_diff_eq(Vec3::NEG_X, 1e-5));
        assert!(dir(Bone::LeftArm, [0.0, 0.0, 90.0]).abs_diff_eq(Vec3::X, 1e-5));
        // Avant-bras plié vers le haut, genou plié vers l'arrière
        assert!(dir(Bone::LeftForearm, [90.0, 0.0, 0.0]).abs_diff_eq(Vec3::Z, 1e-5));
        assert!(dir(Bone::RightShin, [90.0, 0.0, 0.0]).abs_diff_eq(Vec3::NEG_Z, 1e-5));
        // Torse penché en avant, puis vers la gauche ; tourner vers la gauche
        let up = |r: [f32; 3]| Bone::Torso.rotation_from_degrees(r) * Vec3::Y;
        assert!(up([30.0, 0.0, 0.0]).z > 0.4);
        assert!(up([0.0, 0.0, 30.0]).x > 0.4);
        assert!((Bone::Torso.rotation_from_degrees([0.0, 90.0, 0.0]) * Vec3::Z).abs_diff_eq(Vec3::X, 1e-5));
    }

    #[test]
    fn the_rest_pose_keeps_the_limbs_straight() {
        let (_, f) = hero("humain", 0);
        // Rien ne bouge au repos : chaque coin reste où le patron le pose
        let rest: Vec<Vec3> = f.pieces.iter().flat_map(|p| p.quads.iter()).flat_map(|q| q.corners).collect();
        assert!(points(&f).iter().zip(&rest).all(|(a, b)| a.abs_diff_eq(*b, 1e-4)));
        // Bras droit entier à gauche de l'axe, sous l'épaule
        let shoulder = f.pivot(Bone::RightArm);
        assert!(limb(&f, &FigurePose::default(), Bone::RightArm).iter().all(|p| p.x < 0.0 && p.y < shoulder.y + 2.5));
    }

    #[test]
    fn a_bent_elbow_curves_the_arm_without_tearing_it() {
        let (_, f) = hero("humain", 0);
        let mut pose = FigurePose::default();
        pose.bone_mut(Bone::RightForearm).rotation = Bone::RightForearm.rotation_from_degrees([90.0, 0.0, 0.0]);
        let bent = limb(&f, &pose, Bone::RightArm);
        let elbow = f.pivot(Bone::RightForearm);
        // L'avant-bras part vers l'avant à hauteur du coude ; le haut du bras n'a pas bougé
        let reach = bent.iter().fold(f32::MIN, |m, p| m.max(p.z));
        assert!(reach > elbow.y - f.hips - 1.0, "main à {reach} devant");
        let still = limb(&f, &FigurePose::default(), Bone::RightArm);
        assert!(bent.iter().zip(&still).filter(|(_, s)| s.y > elbow.y + 2.1).all(|(b, s)| b.abs_diff_eq(*s, 1e-4)));
        // Pas de déchirure : deux coins confondus au repos le restent une fois pliés, et le dessus
        // du coude s'étire en arc sans qu'une arête ne s'emballe
        let quads: Vec<Quad> = f.posed_with(&pose, 1.0).collect();
        let rest: Vec<Quad> = f.posed(0.0, 1.0).collect();
        let pairs: Vec<(Vec3, Vec3)> = rest.iter().zip(&quads).flat_map(|(r, q)| r.corners.into_iter().zip(q.corners)).collect();
        for (i, (r, q)) in pairs.iter().enumerate().step_by(7) {
            for (r2, q2) in &pairs[i + 1..] {
                if r.abs_diff_eq(*r2, 1e-5) {
                    assert!(q.abs_diff_eq(*q2, 1e-4), "coin {r} déchiré : {q} / {q2}");
                }
            }
        }
        for (q, r) in quads.iter().zip(&rest) {
            for i in 0..4 {
                let (a, b) = ((q.corners[i] - q.corners[(i + 1) % 4]).length(), (r.corners[i] - r.corners[(i + 1) % 4]).length());
                assert!(a < b * 2.2 + 0.05, "arête de {b} étirée à {a}");
            }
        }
    }

    #[test]
    fn a_bent_knee_and_a_twisted_wrist_follow_their_parents() {
        let (_, f) = hero("nain", 0);
        let mut pose = FigurePose::default();
        pose.bone_mut(Bone::LeftShin).rotation = Bone::LeftShin.rotation_from_degrees([90.0, 0.0, 0.0]);
        let shin = limb(&f, &pose, Bone::LeftLeg);
        let knee = f.pivot(Bone::LeftShin);
        // Le pied part en arrière à hauteur du genou
        assert!(shin.iter().fold(f32::MAX, |m, p| m.min(p.z)) < -knee.y + 1.0);
        assert!(shin.iter().all(|p| p.y > knee.y - 3.0));
        // La main suit le bras levé : son repère est celui de l'avant-bras puis du poignet
        let mut raise = FigurePose::default();
        raise.raise_right_arm(std::f32::consts::FRAC_PI_2);
        let frames = f.bone_frames(&raise);
        let hand = frames[Bone::RightHand.index()].apply(f.pivot(Bone::RightHand));
        let shoulder = f.pivot(Bone::RightArm);
        assert!((hand.y - shoulder.y).abs() < 0.01 && hand.z > 5.0, "poignet en {hand}");
        assert!((raise.arm_raise(Bone::RightArm) - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn the_figure_shows_front_faces_only() {
        // De face, la tête montre sa face avant et pas l'arrière
        let head = boite(0.0, 24.0, Vec3::splat(8.0), [8.0, 8.0, 8.0], 0.0, 0.0, 0.0);
        let visible: Vec<usize> = head.iter().enumerate().filter(|(_, q)| q.normal().z > 1e-4).map(|(i, _)| i).collect();
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn boxes_turn_every_face_outwards() {
        let mut quads = Vec::new();
        pave(&mut quads, Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z, [0.0; 4], [true; 6]);
        assert_eq!(quads.len(), 6);
        let centre = Vec3::splat(0.5);
        assert!(quads.iter().all(|q| q.normal().dot(q.centre() - centre) > 0.0));
    }

    #[test]
    fn the_second_layer_stands_out_texel_by_texel() {
        let (_, f) = hero("humain", 0);
        // Bien plus de faces que les six boîtes de la couche 1 : la couche 2 est en pavés
        assert!(f.quad_count() > 36 + 150, "{} faces", f.quad_count());
        // Les cheveux dépassent du crâne
        let top = points(&f).iter().fold(f32::MIN, |t, p| t.max(p.y));
        assert!(top > f.body_height + 0.5, "sommet {top}, crâne {}", f.body_height);
    }

    #[test]
    fn species_keep_their_proportions() {
        let (_, man) = hero("humain", 0);
        let (_, dwarf) = hero("nain", 0);
        let (_, gnome) = hero("gnome", 0);
        // Jambes courtes du nain, grosse tête du gnome
        assert!(dwarf.hips < man.hips - 2.0, "{} / {}", dwarf.hips, man.hips);
        assert!(gnome.body_height - gnome.hips - 12.0 > 9.0, "tête du gnome");
        // Le briseur cornu se voûte, le sylphe flotte
        let (_, horned) = hero("hommeBete", 3);
        assert!(horned.stoop > 0.1, "{}", horned.stoop);
        let (_, sylph) = hero("sylphe", 0);
        assert!(sylph.hover > 0.0);
        let low = points(&sylph).iter().fold(f32::MAX, |l, p| l.min(p.y));
        assert!(low > 1.0, "pieds du sylphe à {low}");
    }

    #[test]
    fn cranial_appendages_rise_above_the_skull() {
        // Porte-cornes : des cornes sur le crâne
        let (_, horns) = hero("demiOrque", 2);
        let (_, plain) = hero("demiOrque", 0);
        assert!(horns.top > horns.body_height + 2.0, "{} / {}", horns.top, horns.body_height);
        assert!(plain.top < plain.body_height + 1.5, "{} / {}", plain.top, plain.body_height);
    }

    #[test]
    fn beast_folk_carry_a_tail_and_aasimar_spread_wings() {
        let (_, tiger) = hero("hommeBete", 0);
        let tail = points(&tiger).into_iter().filter(|p| p.z < -5.5 && p.y < tiger.hips + 2.0).count();
        assert!(tail > 0, "pas de queue");
        let (_, man) = hero("humain", 0);
        assert!(points(&man).iter().all(|p| p.z > -5.5), "un humain n'a ni queue ni ailes");

        let (_, winged) = hero("aasimar", 0);
        let wing = points(&winged).into_iter().filter(|p| p.x.abs() > 6.0 && p.z < -4.0).count();
        assert!(wing > 0, "pas d'ailes");
    }

    #[test]
    fn the_hero_stands_on_his_feet_at_his_height_facing_his_yaw() {
        let (cfg, f) = hero("humain", 0);
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let mut frame = CreatureFrame::default();
        push_hero_posed(&mut frame, &cfg, &f, feet, 0.0, &FigurePose::default(), 1.0);
        assert_eq!(frame.draws.len(), 1);
        assert_eq!(frame.draws[0].0, Some(HERO_TEXTURE));
        assert_eq!(frame.uvs.len(), frame.vertices.len());
        let heights: Vec<f32> = frame.vertices.iter().map(|v| Vec3::from(v.position).length() - feet.length()).collect();
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(*h), hi.max(*h)));
        assert!(low > -0.05 && low < 0.01, "pieds à {low}");
        // Le relief de la couche 2 dépasse un peu la taille du héros
        assert!((high - cfg.taille as f32).abs() < 0.12, "tête à {high}, taille {}", cfg.taille);
        // Le visage (première face de la tête) regarde vers le nord quand le cap est nul
        assert!(frame.vertices[..4].iter().all(|v| (Vec3::from(v.position) - feet).dot(north) > 0.2));
    }

    #[test]
    fn walking_swings_the_limbs_and_standing_still_lets_them_fall() {
        let mut gait = HeroGait::default();
        let mut feet = Vec3::new(0.0, 0.0, 1000.0);
        for _ in 0..60 {
            feet += Vec3::X * (4.0 / 60.0);
            gait.update(feet, 1.0 / 60.0, 1.0);
        }
        assert!(gait.amplitude > 0.6, "{}", gait.amplitude);
        for _ in 0..120 {
            gait.update(feet, 1.0 / 60.0, 1.0);
        }
        assert!(gait.swing().abs() < 0.01);
    }

    /// Nombre de cycles de marche par seconde à l'allure `speed` (blocs par seconde)
    fn cadence(speed: f32) -> f32 {
        let mut gait = HeroGait::default();
        let mut feet = Vec3::new(0.0, 0.0, 1000.0);
        let mut turns = 0.0;
        for _ in 0..600 {
            feet += Vec3::X * (speed / 60.0);
            let before = gait.phase;
            gait.update(feet, 1.0 / 60.0, 1.0);
            turns += (gait.phase - before).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        }
        turns / 10.0
    }

    #[test]
    fn the_limbs_keep_pace_with_the_walk() {
        // Un habitant (1,6 bloc/s) : un peu moins d'un cycle par seconde, soit près de deux pas
        let walk = cadence(1.6);
        assert!((0.8..1.1).contains(&walk), "{walk} cycles par seconde");
        // Plus vite, le pas s'allonge et la cadence monte sans s'emballer
        let run = cadence(5.0);
        assert!(run > walk && run < 2.2, "{run} cycles par seconde");
    }

    /// Les gestes des compétences (`assets/hero/animations/`) ne citent que des os de la figure,
    /// et la frappe au sol, posée par ce code, abat bien la main droite devant le héros
    #[test]
    fn skill_gestures_name_real_bones_and_strike_forward() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/hero/animations");
        let (cfg, f) = hero("humain", 0);
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).expect("dossier des gestes") {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            count += 1;
            let geste: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for key in geste["keys"].as_array().unwrap() {
                let mut pose = FigurePose::default();
                for (name, bone_pose) in key["bones"].as_object().into_iter().flatten() {
                    let bone = Bone::from_name(name).unwrap_or_else(|| panic!("{}: os inconnu {name}", path.display()));
                    let v = |field: &str| bone_pose.get(field).map(|a| {
                        let a = a.as_array().unwrap();
                        [0, 1, 2].map(|i| a[i].as_f64().unwrap() as f32)
                    });
                    if let Some(r) = v("rotation") {
                        pose.bone_mut(bone).rotation = bone.rotation_from_degrees(r);
                    }
                    if let Some(t) = v("translation") {
                        pose.bone_mut(bone).translation = Vec3::from(t) * f.units_per_block(&cfg);
                    }
                }
                let frames = f.bone_frames(&pose);
                assert!(frames.iter().all(|b| b.rotation.is_finite() && b.translation.is_finite()), "{}", path.display());
                if path.ends_with("groundSlam.json") && key["t"].as_f64() == Some(0.35) {
                    let hand = frames[Bone::RightHand.index()].apply(f.pivot(Bone::RightHand));
                    let shoulder = f.pivot(Bone::RightArm);
                    assert!(hand.z > 3.0 && hand.y < shoulder.y, "poignet en {hand}");
                }
            }
        }
        assert!(count >= 15, "{count} gestes");
    }
}
