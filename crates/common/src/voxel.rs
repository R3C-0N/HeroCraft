use serde::{Deserialize, Serialize};

/// Numéro d'un type de bloc, celui de sa fiche (`assets/blocks/`). Les constantes sont les blocs
/// que pose le générateur de terrain ; leurs propriétés sont dans `blocks.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}
