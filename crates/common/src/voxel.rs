use serde::{Deserialize, Serialize};

/// Numéro d'un type de bloc, celui de sa fiche (`assets/blocks/`). Les constantes sont les blocs
/// que pose le générateur de terrain ; leurs propriétés sont dans `blocks.rs`. Par défaut : l'air.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u16);

impl BlockId {
    pub const AIR: BlockId = BlockId(0);
    pub const STONE: BlockId = BlockId(1);
    pub const DIRT: BlockId = BlockId(2);
    pub const GRASS: BlockId = BlockId(3);
    pub const SAND: BlockId = BlockId(4);
    pub const SNOW: BlockId = BlockId(5);
    pub const WATER: BlockId = BlockId(6);
    pub const LAVA: BlockId = BlockId(7);
    pub const WOOD: BlockId = BlockId(8);
    pub const LEAVES: BlockId = BlockId(9);
    pub const IRON_ORE: BlockId = BlockId(10);
    pub const GOLD_ORE: BlockId = BlockId(11);
    pub const CRYSTAL: BlockId = BlockId(12);
    pub const OBSIDIAN: BlockId = BlockId(13);

    /// Temps pour casser le bloc à mains nues, en secondes. Les autres blocs le tirent de la
    /// dureté de leur fiche (1 pour une fleur : 0,1 s ; 14 pour une roche : 1,4 s).
    pub fn break_seconds(&self) -> f32 {
        match self.kind() {
            BlockId::LEAVES | BlockId::SNOW => 0.3,
            BlockId::SAND | BlockId::LAVA => 0.45,
            BlockId::DIRT => 0.5,
            BlockId::GRASS => 0.6,
            BlockId::CRYSTAL => 0.9,
            BlockId::WOOD => 1.2,
            BlockId::STONE => 1.5,
            BlockId::IRON_ORE | BlockId::GOLD_ORE => 2.2,
            BlockId::OBSIDIAN => 5.0,
            other => crate::blocks::blocks().get(other).map_or(0.6, |sheet| (sheet.hardness * 0.1).clamp(0.1, 5.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_break_seconds() {
        assert!(BlockId::DIRT.break_seconds() < BlockId::STONE.break_seconds());
        assert!(BlockId::STONE.break_seconds() < BlockId::OBSIDIAN.break_seconds());
        let flower = crate::blocks::blocks().find("dandelion").unwrap();
        let brick = crate::blocks::blocks().find("brick").unwrap();
        assert!(flower.break_seconds() < brick.break_seconds());
    }
}
