//! Temples à deux époques.
//!
//! Un temple existe au même endroit à deux époques :
//! - **au présent** (`Epoch::Present`), la ruine que le générateur pose déjà parmi les ruines de la
//!   planète : podium, colonnes brisées, autel coiffé d'un cristal. Sous l'autel, une cache creusée
//!   dans le podium attend la Pierre du Néant ;
//! - **dans le passé** (`Epoch::Past`), le même temple intact, tenu en guerre par un peuple : podium
//!   entier, péristyle de colonnes pleines sous une architrave, toit à gradins coiffé d'un cristal,
//!   sanctuaire aux murs de ce peuple, autel doré où repose la Pierre, barricade de pieux sur le
//!   rebord du podium, ouverte devant l'entrée, et lanternes.
//!
//! Devant l'entrée s'étend une arène (`TempleSite::arena`) : un sol de sable une marche sous le
//! temple, cerné d'un mur ouvert contre le podium et percé d'une seule porte au bout opposé. C'est
//! là que se livre le combat final de la vision contre un monstre du Néant (bible, section 8). Au
//! présent, ses murs sont effondrés et l'herbe envahit le sable.
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
//! - mettre le joueur en place : `TempleSite::approach` (devant la porte de l'arène, dehors),
//!   `TempleSite::arena_center` (où attendre le monstre) et `TempleSite::sanctum` (la salle de
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
        TempleId {
            face: self.face,
            x,
            z,
        }
    }

    /// Colonne de l'autel
    pub fn center(&self) -> (i32, i32) {
        self.temple.foot.center()
    }

    /// Colonnes du temple (le podium)
    pub fn foot(&self) -> Rect {
        self.temple.foot
    }

    /// Toutes les colonnes que le temple et son arène changent
    pub fn rect(&self) -> Rect {
        self.temple.rect()
    }

    /// Altitude du sol du temple (premier bloc d'air au-dessus du podium)
    pub fn floor(&self) -> i32 {
        self.temple.floor
    }

    /// Côté de l'entrée
    pub fn door(&self) -> Side {
        self.temple.door()
    }

    /// Emplacement de la Pierre du Néant à l'époque `epoch` : dans le passé sur l'autel doré du
    /// sanctuaire, au présent dans la cache creusée sous l'autel. Le générateur y laisse de l'air.
    pub fn stone_slot(&self, epoch: Epoch) -> BlockPos {
        let (x, y, z) = self.temple.stone_slot(epoch.is_past());
        BlockPos::new(self.face, x, y, z)
    }

    /// L'arène devant l'entrée du temple, où se livre le combat final de la vision
    pub fn arena(&self) -> Rect {
        self.temple.arena
    }

    /// Le centre de l'arène, au niveau de son sol (là où attendre le monstre du Néant)
    pub fn arena_center(&self) -> BlockPos {
        let (x, z) = self.temple.arena.center();
        BlockPos::new(self.face, x, self.temple.arena_floor(), z)
    }

    /// Où se tenir pour aborder le temple : la colonne juste devant la porte de l'arène, dehors,
    /// à la hauteur du sol de l'arène (les pieds y sont à une marche près)
    pub fn approach(&self) -> BlockPos {
        let arena = self.temple.arena;
        let (cx, cz) = self.temple.foot.center();
        let (x, z) = match self.door() {
            Side::North => (cx, arena.z0 - 1),
            Side::South => (cx, arena.z1),
            Side::West => (arena.x0 - 1, cz),
            Side::East => (arena.x1, cz),
        };
        BlockPos::new(self.face, x, self.temple.arena_floor(), z)
    }

    /// La salle du sanctuaire du temple intact (l'intérieur de ses murs), où se dresse l'autel
    pub fn sanctum(&self) -> Rect {
        self.temple.cella().expand(-1)
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
        PlanetWorldGenerator::new(PlanetDescriptor {
            seed,
            ..Default::default()
        })
    }

    /// Un temple de la planète et son générateur
    fn some_temple(seed: u64) -> (PlanetWorldGenerator, TempleSite) {
        let generator = generator(seed);
        let face = PlanetFace::ALL[0];
        let mid = crate::edits::FACE_BLOCKS / 2;
        let site = generator
            .nearest_temple(face, mid, mid, crate::edits::FACE_BLOCKS)
            .expect("au moins un temple sur la face");
        (generator, site)
    }

    fn block(generator: &PlanetWorldGenerator, pos: BlockPos) -> BlockId {
        generator.block_at(pos.face, pos.x, pos.y, pos.z)
    }

    #[test]
    fn nearest_temple_is_found_and_stable() {
        let (generator, site) = some_temple(42);
        let (x, z) = site.center();
        let again = generator
            .temple_at(site.face, x, z)
            .expect("le temple couvre son autel");
        assert_eq!(again.id(), site.id());
        assert_eq!(
            generator
                .nearest_temple(site.face, x, z, 64)
                .map(|t| t.id()),
            Some(site.id())
        );
        assert!(generator
            .temples_touching(site.face, Rect::around(x, z, 2))
            .iter()
            .any(|t| t.id() == site.id()));
        // Une autre partie sur la même graine trouve le même temple
        assert_eq!(some_temple(42).1.id(), site.id());
    }

    #[test]
    fn the_two_epochs_share_the_same_place() {
        let (present, site) = some_temple(42);
        let holder = site.holder_for(Culture::Humains);
        let past = present.with_epoch(Epoch::Past(holder));
        let foot = site.rect();
        let mut differs = 0;
        // Autour du temple, rien ne change ; sur le podium, le temple change de visage
        for z in foot.z0 - 4..foot.z1 + 4 {
            for x in foot.x0 - 4..foot.x1 + 4 {
                for y in site.floor() - 4..site.top() + 2 {
                    let (a, b) = (
                        present.block_at(site.face, x, y, z),
                        past.block_at(site.face, x, y, z),
                    );
                    if a != b {
                        assert!(foot.contains(x, z), "({x}, {y}, {z}) change hors du temple");
                        assert!(y < site.top(), "({x}, {y}, {z}) change au-dessus du temple");
                        differs += 1;
                    }
                }
            }
        }
        assert!(
            differs > 50,
            "le temple intact diffère de la ruine ({differs} blocs)"
        );
    }

    #[test]
    fn the_stone_slot_is_free_and_held() {
        let (present, site) = some_temple(7);
        let past = present.with_epoch(Epoch::Past(site.holder_for(Culture::Nains)));
        for (generator, epoch) in [(&present, Epoch::Present), (&past, past.epoch)] {
            let slot = site.stone_slot(epoch);
            assert!(
                block(generator, slot).is_air(),
                "{epoch:?} : l'emplacement de la Pierre est libre"
            );
            let below = BlockPos {
                y: slot.y - 1,
                ..slot
            };
            assert!(
                !block(generator, below).is_air(),
                "{epoch:?} : la Pierre repose sur quelque chose"
            );
            assert_eq!((slot.x, slot.z), site.center());
        }
        // Au présent, la Pierre est cachée : l'autel la recouvre
        let slot = site.stone_slot(Epoch::Present);
        assert!(!block(
            &present,
            BlockPos {
                y: slot.y + 1,
                ..slot
            }
        )
        .is_air());
        // Dans le passé, elle est sous le toit du sanctuaire, à l'abri des murs
        let slot = site.stone_slot(past.epoch);
        assert!(site.sanctum().contains(slot.x, slot.z));
    }

    #[test]
    fn the_past_temple_is_whole_and_open() {
        let (present, site) = some_temple(42);
        let past = present.with_epoch(Epoch::Past(Culture::Nains));
        let (cx, cz) = site.center();
        // Le toit couvre l'autel
        assert!(
            (site.floor() + 2..site.top()).any(|y| !past.block_at(site.face, cx, y, cz).is_air())
        );
        // On entre : de l'approche jusqu'à l'autel, deux blocs d'air libres au-dessus du sol
        let approach = site.approach();
        let (dx, dz) = site.door().delta();
        let (mut x, mut z) = (approach.x, approach.z);
        while (x, z) != (cx, cz) {
            x -= dx;
            z -= dz;
            if (x, z) == (cx, cz) {
                break;
            }
            let floor = if site.arena().contains(x, z) && !site.foot().contains(x, z) {
                site.floor() - 1
            } else {
                site.floor()
            };
            for y in floor..floor + 2 {
                let b = past.block_at(site.face, x, y, z);
                assert!(!b.is_solid(), "({x}, {y}, {z}) bouche l'entrée : {b:?}");
            }
        }
    }

    #[test]
    fn the_arena_is_closed_but_for_its_gate() {
        let (present, site) = some_temple(42);
        let past = present.with_epoch(Epoch::Past(Culture::HautsElfes));
        let arena = site.arena();
        let floor = site.floor() - 1;
        let (mut open, mut walled) = (0, 0);
        for z in arena.z0..arena.z1 {
            for x in arena.x0..arena.x1 {
                if !(x == arena.x0 || x == arena.x1 - 1 || z == arena.z0 || z == arena.z1 - 1)
                    || site.foot().expand(1).contains(x, z)
                {
                    continue;
                }
                if (floor..floor + 2).all(|y| !past.block_at(site.face, x, y, z).is_solid()) {
                    open += 1;
                } else {
                    walled += 1;
                }
            }
        }
        assert_eq!(
            open, 3,
            "seule la porte de trois blocs s'ouvre dans le mur de l'arène"
        );
        assert!(walled > 30);
        // Le sol de l'arène est plat, une marche sous le temple
        let center = site.arena_center();
        assert!(past
            .block_at(site.face, center.x, center.y, center.z)
            .is_air());
        assert!(!past
            .block_at(site.face, center.x, center.y - 1, center.z)
            .is_air());
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
        assert!(chunks.iter().any(|c| c.x == slot.x.div_euclid(size)
            && c.y == slot.z.div_euclid(size)
            && c.altitude_layer == slot.y.div_euclid(size)));
    }
}
