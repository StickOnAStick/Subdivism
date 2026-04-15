use super::{
    inventory::InventorySlot,
    world::Block,
};

pub struct HudFormatter;

impl HudFormatter {
    pub fn block_label(block: Block) -> &'static str {
        match block {
            Block::Air => "AIR",
            Block::Grass => "GRASS",
            Block::Dirt => "DIRT",
            Block::Stone => "STONE",
            Block::Deepslate => "DEEPSLATE",
            Block::DeepDark => "DEEPDARK",
        }
    }

    pub fn block_short_code(block: Block) -> &'static str {
        match block {
            Block::Air => "_",
            Block::Grass => "G",
            Block::Dirt => "D",
            Block::Stone => "S",
            Block::Deepslate => "L",
            Block::DeepDark => "K",
        }
    }

    pub fn block_tint_color(block: Block) -> [f32; 4] {
        match block {
            Block::Air => [0.15, 0.17, 0.18, 0.35],
            Block::Grass => [0.34, 0.72, 0.36, 0.95],
            Block::Dirt => [0.42, 0.29, 0.20, 0.95],
            Block::Stone => [0.56, 0.58, 0.60, 0.95],
            Block::Deepslate => [0.35, 0.37, 0.40, 0.95],
            Block::DeepDark => [0.08, 0.10, 0.11, 0.95],
        }
    }

    pub fn format_inventory_row(slots: &[InventorySlot], start: usize, width: usize) -> String {
        let mut out = String::new();
        for i in 0..width {
            let idx = start + i;
            let slot = slots.get(idx).copied().unwrap_or_default();
            let token = if slot.is_empty() {
                "___".to_string()
            } else {
                format!(
                    "{}{:02}",
                    Self::block_short_code(slot.block),
                    slot.count.min(99)
                )
            };
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&token);
        }
        out
    }
}
