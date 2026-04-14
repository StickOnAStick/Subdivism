use super::world::Block;

#[derive(Clone, Copy)]
pub struct InventorySlot {
    pub block: Block,
    pub count: u32,
}

pub struct Inventory {
    slots: [InventorySlot; 3],
    selected_index: usize,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            slots: [
                InventorySlot {
                    block: Block::Grass,
                    count: 0,
                },
                InventorySlot {
                    block: Block::Dirt,
                    count: 0,
                },
                InventorySlot {
                    block: Block::Stone,
                    count: 32,
                },
            ],
            selected_index: 2,
        }
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.slots.len() {
            self.selected_index = index;
        }
    }

    pub fn selected_slot(&self) -> InventorySlot {
        self.slots[self.selected_index]
    }

    pub fn slots(&self) -> &[InventorySlot; 3] {
        &self.slots
    }

    pub fn add_block(&mut self, block: Block) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.block == block) {
            slot.count = slot.count.saturating_add(1);
        }
    }

    pub fn try_take_selected(&mut self) -> Option<Block> {
        let slot = &mut self.slots[self.selected_index];
        if slot.count == 0 {
            return None;
        }
        slot.count -= 1;
        Some(slot.block)
    }
}
