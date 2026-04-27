use std::{collections::HashMap, sync::RwLockReadGuard};

use glam::Vec3;

use super::*;

#[derive(Clone, Default)]
pub(super) struct SubOverrideState {
    by_pos: HashMap<SubBlockPos, Block>,
    by_cell: HashMap<BlockPos, Vec<SubBlockPos>>,
}

impl SubOverrideState {
    fn insert(&mut self, pos: SubBlockPos, block: Block) {
        if !self.by_pos.contains_key(&pos) {
            let cell = BlockPos {
                x: pos.x,
                y: pos.y,
                z: pos.z,
            };
            self.by_cell.entry(cell).or_default().push(pos);
        }
        self.by_pos.insert(pos, block);
    }

    fn remove(&mut self, pos: &SubBlockPos) -> Option<Block> {
        let removed = self.by_pos.remove(pos)?;
        let cell = BlockPos {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        };
        if let Some(list) = self.by_cell.get_mut(&cell) {
            list.retain(|entry| entry != pos);
            if list.is_empty() {
                self.by_cell.remove(&cell);
            }
        }
        Some(removed)
    }

    fn remove_cell(&mut self, x: i64, y: i32, z: i64) {
        let cell = BlockPos { x, y, z };
        let Some(entries) = self.by_cell.remove(&cell) else {
            return;
        };
        for pos in entries {
            self.by_pos.remove(&pos);
        }
    }

    fn get(&self, pos: &SubBlockPos) -> Option<Block> {
        self.by_pos.get(pos).copied()
    }

    fn entries_in_cell(&self, x: i64, y: i32, z: i64) -> Vec<(SubBlockPos, Block)> {
        let key = BlockPos { x, y, z };
        let Some(positions) = self.by_cell.get(&key) else {
            return Vec::new();
        };
        positions
            .iter()
            .filter_map(|pos| self.by_pos.get(pos).copied().map(|block| (*pos, block)))
            .collect()
    }

    fn for_each_entry_in_cell<F>(&self, x: i64, y: i32, z: i64, mut visit: F)
    where
        F: FnMut(SubBlockPos, Block) -> bool,
    {
        let key = BlockPos { x, y, z };
        let Some(positions) = self.by_cell.get(&key) else {
            return;
        };
        for pos in positions {
            let Some(block) = self.by_pos.get(pos).copied() else {
                continue;
            };
            if !visit(*pos, block) {
                break;
            }
        }
    }

    pub(super) fn iter_all(&self) -> impl Iterator<Item = (&SubBlockPos, &Block)> {
        self.by_pos.iter()
    }

    pub(super) fn has_entries_in_cell(&self, x: i64, y: i32, z: i64) -> bool {
        self.by_cell.contains_key(&BlockPos { x, y, z })
    }
}

#[derive(Clone)]
pub(super) struct WorldStorage {
    pub(super) overrides: std::sync::Arc<std::sync::RwLock<HashMap<BlockPos, Block>>>,
    pub(super) sub_overrides: std::sync::Arc<std::sync::RwLock<SubOverrideState>>,
    pub(super) spawn_point: Vec3,
    pub(super) center_column: (i32, i32),
}

pub struct WorldCollisionView<'a> {
    world: &'a World,
    overrides: RwLockReadGuard<'a, HashMap<BlockPos, Block>>,
    sub_overrides: RwLockReadGuard<'a, SubOverrideState>,
}

impl World {
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.is_solid_i64(x as i64, y, z as i64)
    }

    pub fn is_solid_i64(&self, x: i64, y: i32, z: i64) -> bool {
        self.block_at(x, y, z).is_solid()
    }

    pub fn block_at_i64(&self, x: i64, y: i32, z: i64) -> Block {
        self.block_at(x, y, z)
    }

    pub fn set_block_i64(&mut self, x: i64, y: i32, z: i64, block: Block) {
        if !contains_world_y(y) {
            return;
        }
        self.storage
            .overrides
            .write()
            .expect("overrides write lock poisoned")
            .insert(BlockPos { x, y, z }, block);
        self.storage
            .sub_overrides
            .write()
            .expect("sub overrides write lock poisoned")
            .remove_cell(x, y, z);
    }

    pub fn set_sub_block_i64(
        &mut self,
        x: i64,
        y: i32,
        z: i64,
        divisions: u8,
        sx: u8,
        sy: u8,
        sz: u8,
        block: Block,
    ) -> bool {
        if !contains_world_y(y) || !valid_sub_coords(divisions, sx, sy, sz) {
            return false;
        }
        let pos = SubBlockPos {
            x,
            y,
            z,
            divisions,
            sx,
            sy,
            sz,
        };
        self.storage
            .sub_overrides
            .write()
            .expect("sub overrides write lock poisoned")
            .insert(pos, block);
        true
    }

    pub fn clear_sub_block_i64(
        &mut self,
        x: i64,
        y: i32,
        z: i64,
        divisions: u8,
        sx: u8,
        sy: u8,
        sz: u8,
    ) -> Option<Block> {
        if !contains_world_y(y) || !valid_sub_coords(divisions, sx, sy, sz) {
            return None;
        }
        let pos = SubBlockPos {
            x,
            y,
            z,
            divisions,
            sx,
            sy,
            sz,
        };
        self.storage
            .sub_overrides
            .write()
            .expect("sub overrides write lock poisoned")
            .remove(&pos)
    }

    pub fn sub_block_i64(
        &self,
        x: i64,
        y: i32,
        z: i64,
        divisions: u8,
        sx: u8,
        sy: u8,
        sz: u8,
    ) -> Option<Block> {
        if !contains_world_y(y) || !valid_sub_coords(divisions, sx, sy, sz) {
            return None;
        }
        let pos = SubBlockPos {
            x,
            y,
            z,
            divisions,
            sx,
            sy,
            sz,
        };
        self.storage
            .sub_overrides
            .read()
            .expect("sub overrides read lock poisoned")
            .get(&pos)
    }

    pub fn sub_blocks_in_cell(&self, x: i64, y: i32, z: i64) -> Vec<(SubBlockPos, Block)> {
        if !contains_world_y(y) {
            return Vec::new();
        }
        self.storage
            .sub_overrides
            .read()
            .expect("sub overrides read lock poisoned")
            .entries_in_cell(x, y, z)
    }

    pub fn for_each_sub_block_in_cell<F>(&self, x: i64, y: i32, z: i64, visit: F)
    where
        F: FnMut(SubBlockPos, Block) -> bool,
    {
        if !contains_world_y(y) {
            return;
        }
        self.storage
            .sub_overrides
            .read()
            .expect("sub overrides read lock poisoned")
            .for_each_entry_in_cell(x, y, z, visit);
    }

    pub fn collision_view(&self) -> WorldCollisionView<'_> {
        WorldCollisionView {
            world: self,
            overrides: self
                .storage
                .overrides
                .read()
                .expect("overrides read lock poisoned"),
            sub_overrides: self
                .storage
                .sub_overrides
                .read()
                .expect("sub overrides read lock poisoned"),
        }
    }

    pub fn spawn_point_for_column(&self, x: i32, z: i32) -> Option<Vec3> {
        let x = x as i64;
        let z = z as i64;
        for y in (WORLD_MIN_Y..=WORLD_MAX_Y).rev() {
            if self.block_at(x, y, z).is_solid() {
                return Some(Vec3::new(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5));
            }
        }
        Some(Vec3::new(
            x as f32 + 0.5,
            (WORLD_OVERWORLD_FLOOR + 2) as f32,
            z as f32 + 0.5,
        ))
    }

    pub(super) fn block_at(&self, x: i64, y: i32, z: i64) -> Block {
        let overrides = self
            .storage
            .overrides
            .read()
            .expect("overrides read lock poisoned");
        self.block_at_with_overrides(&overrides, x, y, z)
    }

    pub(super) fn block_at_with_overrides(
        &self,
        overrides: &HashMap<BlockPos, Block>,
        x: i64,
        y: i32,
        z: i64,
    ) -> Block {
        if !contains_world_y(y) {
            return Block::Air;
        }
        let key = BlockPos { x, y, z };
        if let Some(block) = overrides.get(&key).copied() {
            return block;
        }
        self.procedural_block(x, y, z)
    }
}

impl WorldCollisionView<'_> {
    pub fn is_solid_i64(&self, x: i64, y: i32, z: i64) -> bool {
        self.world
            .block_at_with_overrides(&self.overrides, x, y, z)
            .is_solid()
    }

    pub fn for_each_sub_block_in_cell<F>(&self, x: i64, y: i32, z: i64, visit: F)
    where
        F: FnMut(SubBlockPos, Block) -> bool,
    {
        if !contains_world_y(y) {
            return;
        }
        self.sub_overrides.for_each_entry_in_cell(x, y, z, visit);
    }

    pub fn is_out_of_bounds(&self, position: Vec3, margin: f32) -> bool {
        self.world.is_out_of_bounds(position, margin)
    }

    pub fn spawn_point(&self) -> Vec3 {
        self.world.spawn_point()
    }
}
