use serde::{Deserialize, Serialize};

/// Identifiant unique d'un type de bloc (sur 16 bits = 65 536 types possibles)
/// (par défaut : l'air)
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

    #[inline(always)]
    pub fn is_air(&self) -> bool {
        self.0 == 0
    }

    /// Temps pour casser le bloc à mains nues, en secondes
    pub fn break_seconds(&self) -> f32 {
        match *self {
            BlockId::LEAVES | BlockId::SNOW => 0.3,
            BlockId::SAND | BlockId::LAVA => 0.45,
            BlockId::DIRT => 0.5,
            BlockId::GRASS => 0.6,
            BlockId::CRYSTAL => 0.9,
            BlockId::WOOD => 1.2,
            BlockId::STONE => 1.5,
            BlockId::IRON_ORE | BlockId::GOLD_ORE => 2.2,
            BlockId::OBSIDIAN => 5.0,
            _ => 0.6,
        }
    }

    #[inline(always)]
    pub fn is_opaque(&self) -> bool {
        match self.0 {
            0 => false, // Air
            6 => false, // Water
            9 => false, // Leaves (semi-transparent)
            _ => true,
        }
    }

    #[inline(always)]
    pub fn is_solid(&self) -> bool {
        match self.0 {
            0 => false, // Air
            6 => false, // Water
            7 => false, // Lava
            _ => true,
        }
    }
}

/// Définition détaillée d'un bloc avec ses propriétés physiques et RPG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDefinition {
    pub id: BlockId,
    pub name: String,
    pub hardness: f32,
    pub blast_resistance: f32,
    pub light_emission: u8,
    pub required_tool_tier: u8,
}

impl BlockDefinition {
    pub fn standard(id: BlockId, name: &str, hardness: f32) -> Self {
        Self {
            id,
            name: name.to_string(),
            hardness,
            blast_resistance: hardness * 1.5,
            light_emission: 0,
            required_tool_tier: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_properties() {
        assert!(BlockId::AIR.is_air());
        assert!(!BlockId::STONE.is_air());
        assert!(BlockId::STONE.is_solid());
        assert!(!BlockId::WATER.is_solid());
        assert!(BlockId::STONE.is_opaque());
        assert!(!BlockId::WATER.is_opaque());
        assert!(BlockId::DIRT.break_seconds() < BlockId::STONE.break_seconds());
        assert!(BlockId::STONE.break_seconds() < BlockId::OBSIDIAN.break_seconds());
    }
}
