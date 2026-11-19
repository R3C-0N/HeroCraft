//! Temples à deux époques.
//!
//! Un temple est rond. Une base haute fait le tour de son enceinte, et on n'y entre que par
//! l'avant. Des colonnes posées sur la base, un peu plus hautes vers le fond, sont reliées par une
//! couronne. L'intérieur de l'enceinte est l'arène où se livre le combat final de la vision contre
//! un monstre du Néant (bible, section 8). Au fond, derrière un mur courbe bombé vers l'arène, le
//! sanctuaire est fermé : on n'y entre que par sa porte, depuis l'arène, et autour de lui le tour
//! de l'enceinte devient un mur plein qui monte jusqu'à la couronne. L'autel se dresse au centre du
//! sanctuaire, sous un puits de lumière. Devant l'entrée, un escalier descend jusqu'au sol naturel
//! et s'ouvre sur un parvis encadré de deux braseros. Au centre de l'arène, une mosaïque aux
//! couleurs du peuple marque l'endroit où paraît le monstre.
//!
//! Il existe au même endroit à deux époques :
//! - **au présent** (`Epoch::Present`), c'est une ruine. La base et les colonnes sont brisées, la
//!   couronne et le toit sont tombés, des tambours de colonnes jonchent l'arène et ses abords, le
//!   lierre et la mousse couvrent les pierres, l'herbe perce les dalles et fend la mosaïque. Sous
//!   l'autel, une cache creusée dans le sol du sanctuaire attend la Pierre du Néant, et des éclats
//!   du Néant percent le sol autour d'elle ;
//! - **dans le passé** (`Epoch::Past`), c'est le même temple intact, tenu en guerre par un peuple,
//!   avec les murs et les lanternes de ce peuple, des braseros allumés au sommet des colonnes, et
//!   la Pierre posée sur l'autel doré.
//!
//! Les deux époques changent exactement les mêmes colonnes et les mêmes altitudes : passer de l'une
//! à l'autre ne demande que de régénérer les chunks du temple (`TempleSite::chunks`) avec un
//! générateur réglé sur l'autre époque (`PlanetWorldGenerator::with_epoch`).
//!
//! Points d'accroche pour la scène d'ouverture :
//! - trouver le temple : `PlanetWorldGenerator::nearest_temple`, `temple_at`, `temples_touching` ;
//! - choisir qui le tient dans la vision : `TempleSite::holder_for` (un peuple autre que celui du
//!   joueur), puis `Epoch::Past(peuple)` ;
//! - poser la Pierre : `TempleSite::stone_slot(epoch)`, un bloc d'air réservé à elle ;
//! - mettre le joueur en place : `TempleSite::approach` (au bord du parvis, face à l'escalier),
//!   `TempleSite::arena_center` (où attendre le monstre) et `TempleSite::in_sanctum` (la salle de
//!   l'autel).

use herocraft_common::PlanetFace;

use crate::culture::Culture;
use crate::edits::BlockPos;
use crate::generator::PlanetWorldGenerator;
use crate::structures::{
    mix, Piece, Rect, Settlement, SettlementKind, Side, Temple, CELL, CELLS_PER_FACE,
};
use herocraft_common::PlanetChunkCoord;
use herocraft_voxel_engine::CHUNK_SIZE;

/// Époque à laquelle le générateur dessine les temples
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Epoch {
    /// Le monde d'aujourd'hui : les temples sont en ruine
    #[default]
    Present,
    /// Le temps des guerres entre les nations, vu en vision : les temples sont intacts et tenus par
    /// le peuple donné
    Past(Culture),
}

impl Epoch {
    pub fn is_past(self) -> bool {
        matches!(self, Epoch::Past(_))
    }
}

/// Un temple de la planète, tel que les deux époques le partagent
#[derive(Debug, Clone, Copy)]
pub struct TempleSite {
    pub face: PlanetFace,
    pub temple: Temple,
}

/// Identifiant stable d'un temple : sa face et la colonne de son autel. Le même d'une partie à
/// l'autre pour une même graine, et le même aux deux époques.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TempleId {
    pub face: PlanetFace,
    pub x: i32,
    pub z: i32,
}

impl TempleSite {
    fn of(settlement: &Settlement) -> Option<TempleSite> {
        if settlement.kind != SettlementKind::Ruins {
            return None;
        }
        settlement.pieces.iter().find_map(|p| match p {
            Piece::Temple(t) => Some(TempleSite {
                face: settlement.face,
                temple: *t,
            }),
            _ => None,
        })
    }

    pub fn id(&self) -> TempleId {
        let (x, z) = self.center();
        TempleId { face: self.face, x, z }
    }

    /// Colonne du centre de l'enceinte
    pub fn center(&self) -> (i32, i32) {
        self.temple.center
    }

    /// Rayon de l'enceinte, base comprise
    pub fn radius(&self) -> i32 {
        self.temple.radius
    }

    /// Toutes les colonnes que le temple change
    pub fn rect(&self) -> Rect {
        self.temple.rect()
    }

    /// Altitude du sol de l'arène (premier bloc d'air au-dessus du dallage). Le sanctuaire est un
    /// bloc plus haut.
    pub fn floor(&self) -> i32 {
        self.temple.floor
    }

    /// Côté de l'entrée
    pub fn door(&self) -> Side {
        self.temple.door()
    }

    /// Colonne de l'autel, au centre du sanctuaire
    pub fn altar(&self) -> (i32, i32) {
        self.temple.altar()
    }

    /// Emplacement de la Pierre du Néant à l'époque `epoch` : dans le passé sur l'autel doré, sous
    /// le puits de lumière ; au présent dans la cache creusée sous l'autel. Le générateur y laisse
    /// de l'air.
    pub fn stone_slot(&self, epoch: Epoch) -> BlockPos {
        let (x, y, z) = self.temple.stone_slot(epoch.is_past());
        BlockPos::new(self.face, x, y, z)
    }

    /// Le centre de l'arène, au niveau de son sol (là où attendre le monstre du Néant)
    pub fn arena_center(&self) -> BlockPos {
        let (x, z) = self.temple.arena_center();
        BlockPos::new(self.face, x, self.temple.floor, z)
    }

    /// Où se tenir pour aborder le temple : au bord du parvis, sur l'axe, face à l'escalier qui
    /// monte à l'entrée (les pieds sur le dallage du parvis)
    pub fn approach(&self) -> BlockPos {
        let (x, z) = self.temple.approach();
        BlockPos::new(self.face, x, self.temple.parvis_floor(), z)
    }

    /// La colonne (x, z) est dans la salle du sanctuaire, où se dresse l'autel
    pub fn in_sanctum(&self, x: i32, z: i32) -> bool {
        self.temple.in_sanctum(x, z)
    }

    /// Le peuple qui tient le temple dans la vision d'un joueur du peuple `player` : toujours un
    /// autre que le sien, tiré de la graine du temple (le même pour tous les joueurs de ce peuple)
    pub fn holder_for(&self, player: Culture) -> Culture {
        let others: Vec<Culture> = Culture::ALL.into_iter().filter(|&c| c != player).collect();
        others[(mix(self.temple.seed ^ 0x401D_E125) % others.len() as u64) as usize]
    }

    /// Plus haut bloc (exclu) que le temple pose, à l'une ou l'autre époque
    pub fn top(&self) -> i32 {
        self.temple.top()
    }

    /// Les chunks que le temple touche, à toutes les altitudes qu'il change (à régénérer pour
    /// passer d'une époque à l'autre)
    pub fn chunks(&self) -> Vec<PlanetChunkCoord> {
        let size = CHUNK_SIZE as i32;
        let foot = self.temple.rect();
        // Le podium descend au plus jusqu'au sol le plus bas : quelques blocs suffisent en général,
        // la fondation de pierre dure part de `floor - 3`
        let low = (self.temple.floor - 12).max(0).div_euclid(size);
        let high = (self.top() - 1).div_euclid(size);
        let mut found = Vec::new();
        for cz in foot.z0.div_euclid(size)..=(foot.z1 - 1).div_euclid(size) {
            for cx in foot.x0.div_euclid(size)..=(foot.x1 - 1).div_euclid(size) {
                for layer in low..=high {
                    found.push(PlanetChunkCoord::new(self.face, cx, cz, layer));
                }
            }
        }
        found
    }
}

impl PlanetWorldGenerator {
    /// Le même générateur, qui dessine les temples à l'époque `epoch` (il partage les plans des
    /// établissements et les chemins déjà tracés)
    pub fn with_epoch(&self, epoch: Epoch) -> PlanetWorldGenerator {
        PlanetWorldGenerator {
            epoch,
            ..self.clone()
        }
    }

    /// Le temple dont le podium couvre la colonne (x, z) d'une face, s'il y en a un
    pub fn temple_at(&self, face: PlanetFace, x: i32, z: i32) -> Option<TempleSite> {
        let site = TempleSite::of(self.settlement_at(face, x, z)?)?;
        site.temple.rect().contains(x, z).then_some(site)
    }

    /// Les temples dont le podium touche le rectangle de colonnes `area` d'une face
    pub fn temples_touching(&self, face: PlanetFace, area: Rect) -> Vec<TempleSite> {
        self.structures
            .touching(self, face, area)
            .into_iter()
            .filter_map(TempleSite::of)
            .filter(|t| t.temple.rect().intersects(&area))
            .collect()
    }

    /// Le temple le plus proche de la colonne (x, z) d'une face, cherché jusqu'à `max_blocks` blocs
    /// (sur cette face seulement). Il y en a environ un par région de 2 048 blocs de côté.
    pub fn nearest_temple(
        &self,
        face: PlanetFace,
        x: i32,
        z: i32,
        max_blocks: i32,
    ) -> Option<TempleSite> {
        let (cx, cz) = (x.div_euclid(CELL), z.div_euclid(CELL));
        let rings = max_blocks.div_euclid(CELL) + 1;
        let mut best: Option<(i64, TempleSite)> = None;
        for ring in 0..=rings {
            // Une case de l'anneau `ring` est à au moins (ring - 1) × CELL blocs : au-delà du
            // meilleur trouvé, rien de plus proche ne viendra
            if let Some((d2, _)) = best {
                let near = ((ring - 1).max(0) * CELL) as i64;
                if near * near > d2 {
                    break;
                }
            }
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let (nx, nz) = (cx + dx, cz + dz);
                    if !(0..CELLS_PER_FACE).contains(&nx) || !(0..CELLS_PER_FACE).contains(&nz) {
                        continue;
                    }
                    let Some(site) = self
                        .structures
                        .cell(self, face, nx, nz)
                        .and_then(TempleSite::of)
                    else {
                        continue;
                    };
                    let (tx, tz) = site.center();
                    let d2 = ((tx - x) as i64).pow(2) + ((tz - z) as i64).pow(2);
                    if d2 <= (max_blocks as i64).pow(2) && best.is_none_or(|(b, _)| d2 < b) {
                        best = Some((d2, site));
                    }
                }
            }
        }
        best.map(|(_, site)| site)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use herocraft_common::BlockId;

    fn generator(seed: u64) -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed, ..Default::default() })
    }

    /// Un temple de la planète et son générateur
    fn some_temple(seed: u64) -> (PlanetWorldGenerator, TempleSite) {
        let generator = generator(seed);
        let face = PlanetFace::ALL[0];
        let mid = crate::edits::FACE_BLOCKS / 2;
        let site = generator.nearest_temple(face, mid, mid, crate::edits::FACE_BLOCKS).expect("au moins un temple sur la face");
        (generator, site)
    }

    fn block(generator: &PlanetWorldGenerator, pos: BlockPos) -> BlockId {
        generator.block_at(pos.face, pos.x, pos.y, pos.z)
    }

    /// Altitude où l'on se tient debout dans la colonne (x, z), cherchée de `low` à `high`
    fn standing(generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32, low: i32, high: i32) -> Option<i32> {
        (low..=high).find(|&y| generator.block_at(face, x, y - 1, z).is_solid() && !generator.block_at(face, x, y, z).is_solid() && !generator.block_at(face, x, y + 1, z).is_solid())
    }

    /// Distance de la colonne (x, z) à l'axe de l'entrée
    fn off_axis(site: &TempleSite, x: i32, z: i32) -> i32 {
        let (cx, cz) = site.center();
        let (dx, dz) = site.door().delta();
        (dz * (x - cx) - dx * (z - cz)).abs()
    }

    #[test]
    fn nearest_temple_is_found_and_stable() {
        let (generator, site) = some_temple(42);
        let (x, z) = site.center();
        let again = generator.temple_at(site.face, x, z).expect("le temple couvre son centre");
        assert_eq!(again.id(), site.id());
        assert_eq!(generator.nearest_temple(site.face, x, z, 64).map(|t| t.id()), Some(site.id()));
        assert!(generator.temples_touching(site.face, Rect::around(x, z, 2)).iter().any(|t| t.id() == site.id()));
        // Une autre partie sur la même graine trouve le même temple
        assert_eq!(some_temple(42).1.id(), site.id());
    }

    #[test]
    fn the_two_epochs_share_the_same_place() {
        let (present, site) = some_temple(42);
        let past = present.with_epoch(Epoch::Past(site.holder_for(Culture::Humains)));
        let rect = site.rect();
        let mut differs = 0;
        // Autour du temple, rien ne change ; dans l'enceinte, le temple change de visage
        for z in rect.z0 - 4..rect.z1 + 4 {
            for x in rect.x0 - 4..rect.x1 + 4 {
                for y in site.floor() - 4..site.top() + 2 {
                    if present.block_at(site.face, x, y, z) != past.block_at(site.face, x, y, z) {
                        assert!(rect.contains(x, z), "({x}, {y}, {z}) change hors du temple");
                        assert!(y < site.top(), "({x}, {y}, {z}) change au-dessus du temple");
                        differs += 1;
                    }
                }
            }
        }
        assert!(differs > 200, "le temple intact diffère de la ruine ({differs} blocs)");
    }

    #[test]
    fn the_stone_slot_is_free_and_held() {
        let (present, site) = some_temple(7);
        let past = present.with_epoch(Epoch::Past(site.holder_for(Culture::Nains)));
        for (generator, epoch) in [(&present, Epoch::Present), (&past, past.epoch)] {
            let slot = site.stone_slot(epoch);
            assert!(block(generator, slot).is_air(), "{epoch:?} : l'emplacement de la Pierre est libre");
            assert!(!block(generator, BlockPos { y: slot.y - 1, ..slot }).is_air(), "{epoch:?} : la Pierre repose sur quelque chose");
            assert_eq!((slot.x, slot.z), site.altar());
            assert!(site.in_sanctum(slot.x, slot.z));
        }
        // Au présent, la Pierre est cachée : l'autel la recouvre
        let slot = site.stone_slot(Epoch::Present);
        assert!(!block(&present, BlockPos { y: slot.y + 1, ..slot }).is_air());
        // Dans le passé, la lumière tombe sur elle : rien au-dessus jusqu'au ciel
        let slot = site.stone_slot(past.epoch);
        assert!((slot.y..site.top() + 2).all(|y| block(&past, BlockPos { y, ..slot }).is_air()));
        // Et le toit du sanctuaire couvre le reste de la salle
        let (ax, az) = site.altar();
        let (dx, dz) = site.door().delta();
        let (x, z) = (ax - 3 * dz, az + 3 * dx);
        assert!(site.in_sanctum(x, z));
        assert!((site.floor() + 2..site.top()).any(|y| past.block_at(site.face, x, y, z).is_solid()));
    }

    #[test]
    fn one_walks_from_the_entrance_to_the_altar() {
        let (present, site) = some_temple(42);
        let past = present.with_epoch(Epoch::Past(Culture::Nains));
        let (ax, az) = site.altar();
        let (dx, dz) = site.door().delta();
        let start = site.approach();
        let (mut x, mut z) = (start.x - dx, start.z - dz);
        let mut previous = standing(&past, site.face, start.x, start.z, start.y - 1, start.y + 1).expect("on se tient devant l'entrée");
        while (x - dx, z - dz) != (ax, az) {
            let here = standing(&past, site.face, x, z, previous - 1, previous + 1).unwrap_or_else(|| panic!("({x}, {z}) bouche le chemin de l'autel"));
            previous = here;
            x -= dx;
            z -= dz;
        }
    }

    #[test]
    fn the_enclosure_and_the_sanctum_are_closed_but_for_their_doors() {
        let (present, site) = some_temple(42);
        let past = present.with_epoch(Epoch::Past(Culture::HautsElfes));
        let (cx, cz) = site.center();
        let r = site.radius() as f64;
        let free = |x: i32, z: i32, y0: i32| (y0..y0 + 3).all(|y| !past.block_at(site.face, x, y, z).is_solid());
        // Le tour de l'enceinte : on ne passe que par l'entrée
        let rect = site.rect();
        let mut gaps = 0;
        for z in rect.z0..rect.z1 {
            for x in rect.x0..rect.x1 {
                let d = (((x - cx) * (x - cx) + (z - cz) * (z - cz)) as f64).sqrt();
                if d > r - 1.5 && d <= r + 0.5 && free(x, z, site.floor()) {
                    let (dx, dz) = site.door().delta();
                    assert!(off_axis(&site, x, z) <= 1 && dx * (x - cx) + dz * (z - cz) > 0, "brèche dans le tour en ({x}, {z})");
                    gaps += 1;
                }
            }
        }
        assert!(gaps > 0, "l'entrée est ouverte");
        // Le sanctuaire : on n'en sort que par sa porte, vers l'arène
        let mut doors = 0;
        for z in rect.z0..rect.z1 {
            for x in rect.x0..rect.x1 {
                if !site.in_sanctum(x, z) {
                    continue;
                }
                for (nx, nz) in [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)] {
                    if site.in_sanctum(nx, nz) {
                        continue;
                    }
                    // Le voisin est le mur : il ne laisse passer qu'à la porte
                    if free(nx, nz, site.floor() + 1) {
                        assert!(off_axis(&site, nx, nz) <= 1, "brèche dans le sanctuaire en ({nx}, {nz})");
                        doors += 1;
                    }
                }
            }
        }
        assert!(doors > 0, "la porte du sanctuaire est ouverte");
    }

    #[test]
    fn the_holder_is_never_the_players_people() {
        let (_, site) = some_temple(42);
        for player in Culture::ALL {
            assert_ne!(site.holder_for(player), player);
            assert_eq!(site.holder_for(player), site.holder_for(player));
        }
    }

    #[test]
    fn the_chunks_cover_the_temple() {
        let (_, site) = some_temple(42);
        let chunks = site.chunks();
        let size = CHUNK_SIZE as i32;
        let slot = site.stone_slot(Epoch::Present);
        assert!(chunks.iter().any(|c| c.x == slot.x.div_euclid(size) && c.y == slot.z.div_euclid(size) && c.altitude_layer == slot.y.div_euclid(size)));
    }
}
