use super::world::Block;

pub const HOTBAR_SIZE: usize = 8;
pub const BACKPACK_ROWS: usize = 6;
pub const BACKPACK_COLS: usize = 8;
pub const BACKPACK_SIZE: usize = BACKPACK_ROWS * BACKPACK_COLS;
const STACK_LIMIT: u32 = 99;

#[derive(Clone, Copy)]
pub struct InventorySlot {
    pub block: Block,
    pub count: u32,
}

impl Default for InventorySlot {
    fn default() -> Self {
        Self {
            block: Block::Air,
            count: 0,
        }
    }
}

impl InventorySlot {
    pub fn is_empty(self) -> bool {
        self.count == 0 || matches!(self.block, Block::Air)
    }
}

pub struct Inventory {
    hotbar: [InventorySlot; HOTBAR_SIZE],
    backpack: [InventorySlot; BACKPACK_SIZE],
    selected_hotbar: usize,
}

impl Inventory {
    pub fn new() -> Self {
        let mut hotbar = [InventorySlot::default(); HOTBAR_SIZE];
        hotbar[0] = InventorySlot {
            block: Block::Stone,
            count: 32,
        };
        hotbar[1] = InventorySlot {
            block: Block::Grass,
            count: 16,
        };
        hotbar[2] = InventorySlot {
            block: Block::Dirt,
            count: 16,
        };
        Self {
            hotbar,
            backpack: [InventorySlot::default(); BACKPACK_SIZE],
            selected_hotbar: 0,
        }
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.hotbar.len() {
            self.selected_hotbar = index;
        }
    }

    pub fn selected_hotbar_index(&self) -> usize {
        self.selected_hotbar
    }

    pub fn selected_slot(&self) -> InventorySlot {
        self.hotbar[self.selected_hotbar]
    }

    pub fn hotbar(&self) -> &[InventorySlot; HOTBAR_SIZE] {
        &self.hotbar
    }

    pub fn backpack(&self) -> &[InventorySlot; BACKPACK_SIZE] {
        &self.backpack
    }

    pub fn backpack_slot(&self, index: usize) -> Option<InventorySlot> {
        self.backpack.get(index).copied()
    }

    pub fn filled_slots(&self) -> usize {
        self.hotbar
            .iter()
            .chain(self.backpack.iter())
            .filter(|slot| !slot.is_empty())
            .count()
    }

    pub fn slot_capacity(&self) -> usize {
        HOTBAR_SIZE + BACKPACK_SIZE
    }

    pub fn add_block(&mut self, block: Block) -> bool {
        if matches!(block, Block::Air) {
            return false;
        }
        if try_insert_existing_stack(&mut self.hotbar, block) {
            return true;
        }
        if try_insert_empty_slot(&mut self.hotbar, block) {
            return true;
        }
        if try_insert_existing_stack(&mut self.backpack, block) {
            return true;
        }
        try_insert_empty_slot(&mut self.backpack, block)
    }

    pub fn try_take_selected(&mut self) -> Option<Block> {
        let slot = &mut self.hotbar[self.selected_hotbar];
        if slot.is_empty() {
            return None;
        }
        slot.count -= 1;
        let block = slot.block;
        if slot.count == 0 {
            slot.block = Block::Air;
        }
        Some(block)
    }

    pub fn try_take_selected_matching(&mut self, expected: Block) -> bool {
        let slot = &mut self.hotbar[self.selected_hotbar];
        if slot.is_empty() || slot.block != expected {
            return false;
        }
        slot.count -= 1;
        if slot.count == 0 {
            slot.block = Block::Air;
        }
        true
    }

    pub fn move_backpack_slot_to_hotbar(
        &mut self,
        backpack_index: usize,
        hotbar_index: usize,
    ) -> bool {
        if backpack_index >= self.backpack.len() || hotbar_index >= self.hotbar.len() {
            return false;
        }

        let from = self.backpack[backpack_index];
        if from.is_empty() {
            return false;
        }
        let to = self.hotbar[hotbar_index];

        if to.is_empty() {
            self.hotbar[hotbar_index] = from;
            self.backpack[backpack_index] = InventorySlot::default();
            return true;
        }

        if to.block == from.block && to.count < STACK_LIMIT {
            let can_move = STACK_LIMIT - to.count;
            let moved = can_move.min(from.count);
            self.hotbar[hotbar_index].count += moved;
            self.backpack[backpack_index].count -= moved;
            if self.backpack[backpack_index].count == 0 {
                self.backpack[backpack_index] = InventorySlot::default();
            }
            return true;
        }

        self.hotbar[hotbar_index] = from;
        self.backpack[backpack_index] = to;
        true
    }
}

fn try_insert_existing_stack(slots: &mut [InventorySlot], block: Block) -> bool {
    if let Some(slot) = slots
        .iter_mut()
        .find(|slot| slot.block == block && slot.count < STACK_LIMIT)
    {
        slot.count += 1;
        return true;
    }
    false
}

fn try_insert_empty_slot(slots: &mut [InventorySlot], block: Block) -> bool {
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_empty()) {
        slot.block = block;
        slot.count = 1;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_pickup_prefers_hotbar_then_backpack() {
        let mut inventory = Inventory::new();
        for slot in inventory.hotbar.iter_mut() {
            slot.block = Block::Stone;
            slot.count = STACK_LIMIT;
        }
        for slot in inventory.backpack.iter_mut() {
            slot.block = Block::Air;
            slot.count = 0;
        }

        assert!(inventory.add_block(Block::Grass));
        let backpack_used = inventory
            .backpack
            .iter()
            .any(|slot| slot.block == Block::Grass && slot.count == 1);
        assert!(backpack_used);
    }

    #[test]
    fn moving_backpack_stack_to_hotbar_swaps_or_merges() {
        let mut inventory = Inventory::new();
        inventory.hotbar[0] = InventorySlot {
            block: Block::Dirt,
            count: 10,
        };
        inventory.backpack[0] = InventorySlot {
            block: Block::Dirt,
            count: 5,
        };

        assert!(inventory.move_backpack_slot_to_hotbar(0, 0));
        assert_eq!(inventory.hotbar[0].count, 15);
        assert!(inventory.backpack[0].is_empty());
    }
}
