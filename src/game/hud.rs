use super::{inventory::InventorySlot, world::Block};

pub struct HudFormatter;

impl HudFormatter {
    pub fn block_label(block: Block) -> &'static str {
        block.label()
    }

    pub fn block_short_code(block: Block) -> &'static str {
        block.short_code()
    }

    pub fn block_tint_color(block: Block) -> [f32; 4] {
        block.hud_tint()
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
