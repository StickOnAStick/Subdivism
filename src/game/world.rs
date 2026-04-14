use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use glam::Vec3;

use crate::mesh::{push_quad, Vertex};

pub const CHUNK_SIZE: i64 = 16;
pub const WORLD_HEIGHT: i32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
}

impl Block {
    pub fn is_solid(self) -> bool {
        !matches!(self, Self::Air)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: i64,
    pub y: i32,
    pub z: i64,
}

#[derive(Clone)]
pub struct World {
    overrides: Arc<RwLock<HashMap<BlockPos, Block>>>,
    spawn_point: Vec3,
    seed: i64,
}

impl World {
    pub fn generate(_size_x: i32, _size_y: i32, _size_z: i32) -> Self {
        let seed = 0x5EED_BA5E_u64 as i64;
        let mut world = Self {
            overrides: Arc::new(RwLock::new(HashMap::new())),
            spawn_point: Vec3::ZERO,
            seed,
        };
        world.spawn_point = world
            .spawn_point_for_column(0, 0)
            .unwrap_or(Vec3::new(0.5, 12.0, 0.5));
        world
    }

    pub fn spawn_point(&self) -> Vec3 {
        self.spawn_point
    }

    pub fn is_out_of_bounds(&self, position: Vec3, margin: f32) -> bool {
        position.y < -margin
    }

    pub fn world_to_chunk(x: i64, z: i64) -> (i64, i64) {
        (div_floor(x, CHUNK_SIZE), div_floor(z, CHUNK_SIZE))
    }

    pub fn center_column(&self) -> (i32, i32) {
        (0, 0)
    }

    pub fn build_chunk_mesh(&self, chunk: (i64, i64)) -> Vec<Vertex> {
        let overrides = self.overrides.read().expect("overrides read lock poisoned");
        self.build_chunk_mesh_with_overrides(chunk, &overrides)
    }

    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.is_solid_i64(x as i64, y, z as i64)
    }

    pub fn is_solid_i64(&self, x: i64, y: i32, z: i64) -> bool {
        self.block_at(x, y, z).is_solid()
    }

    pub fn set_block_i64(&mut self, x: i64, y: i32, z: i64, block: Block) {
        if !(0..WORLD_HEIGHT).contains(&y) {
            return;
        }
        self.overrides
            .write()
            .expect("overrides write lock poisoned")
            .insert(BlockPos { x, y, z }, block);
    }

    pub fn spawn_point_for_column(&self, x: i32, z: i32) -> Option<Vec3> {
        let x = x as i64;
        let z = z as i64;
        for y in (0..WORLD_HEIGHT).rev() {
            if self.block_at(x, y, z).is_solid() {
                return Some(Vec3::new(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5));
            }
        }
        Some(Vec3::new(x as f32 + 0.5, 1.0, z as f32 + 0.5))
    }

    fn build_chunk_mesh_with_overrides(
        &self,
        chunk: (i64, i64),
        overrides: &HashMap<BlockPos, Block>,
    ) -> Vec<Vertex> {
        let mut vertices = Vec::new();
        let base_x = chunk.0 * CHUNK_SIZE;
        let base_z = chunk.1 * CHUNK_SIZE;

        for local_z in 0..CHUNK_SIZE {
            for local_x in 0..CHUNK_SIZE {
                let world_x = base_x + local_x;
                let world_z = base_z + local_z;
                let surface_y = self.surface_height(world_x, world_z);
                for y in 0..=surface_y {
                    let block = self.block_at_with_overrides(overrides, world_x, y, world_z);
                    if !block.is_solid() {
                        continue;
                    }

                    let base = Vec3::new(world_x as f32, y as f32, world_z as f32);
                    let (top_color, side_color, bottom_color) = palette(block, world_x, world_z);

                    if !self
                        .block_at_with_overrides(overrides, world_x, y + 1, world_z)
                        .is_solid()
                    {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            top_color,
                        );
                    }
                    if y > 0
                        && !self
                            .block_at_with_overrides(overrides, world_x, y - 1, world_z)
                            .is_solid()
                    {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                            ],
                            bottom_color,
                        );
                    }
                    if !self
                        .block_at_with_overrides(overrides, world_x - 1, y, world_z)
                        .is_solid()
                    {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                            ],
                            side_color,
                        );
                    }
                    if !self
                        .block_at_with_overrides(overrides, world_x + 1, y, world_z)
                        .is_solid()
                    {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                            ],
                            side_color,
                        );
                    }
                    if !self
                        .block_at_with_overrides(overrides, world_x, y, world_z - 1)
                        .is_solid()
                    {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            side_color,
                        );
                    }
                    if !self
                        .block_at_with_overrides(overrides, world_x, y, world_z + 1)
                        .is_solid()
                    {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                            ],
                            side_color,
                        );
                    }
                }
            }
        }

        vertices
    }

    fn block_at(&self, x: i64, y: i32, z: i64) -> Block {
        let overrides = self.overrides.read().expect("overrides read lock poisoned");
        self.block_at_with_overrides(&overrides, x, y, z)
    }

    fn block_at_with_overrides(
        &self,
        overrides: &HashMap<BlockPos, Block>,
        x: i64,
        y: i32,
        z: i64,
    ) -> Block {
        if !(0..WORLD_HEIGHT).contains(&y) {
            return Block::Air;
        }
        let key = BlockPos { x, y, z };
        if let Some(block) = overrides.get(&key).copied() {
            return block;
        }
        self.procedural_block(x, y, z)
    }

    fn procedural_block(&self, x: i64, y: i32, z: i64) -> Block {
        let surface_y = self.surface_height(x, z);
        if y > surface_y {
            return Block::Air;
        }
        if y == surface_y {
            return Block::Grass;
        }
        if y >= surface_y - 2 {
            return Block::Dirt;
        }
        Block::Stone
    }

    fn surface_height(&self, x: i64, z: i64) -> i32 {
        let fx = x as f32;
        let fz = z as f32;
        let s = self.seed as f32 * 0.0001;
        let terrain = ((fx * 0.035 + s).sin() * 6.5
            + (fz * 0.028 + s * 2.0).cos() * 5.0
            + ((fx + fz) * 0.012).sin() * 3.0)
            .round() as i32;
        (WORLD_HEIGHT / 2 + terrain).clamp(2, WORLD_HEIGHT - 4)
    }
}

fn div_floor(value: i64, divisor: i64) -> i64 {
    let mut result = value / divisor;
    let remainder = value % divisor;
    if remainder != 0 && (remainder > 0) != (divisor > 0) {
        result -= 1;
    }
    result
}

fn palette(block: Block, x: i64, z: i64) -> ([f32; 3], [f32; 3], [f32; 3]) {
    match block {
        Block::Grass => {
            let top = if ((x + z) & 1) == 0 {
                [0.36, 0.78, 0.40]
            } else {
                [0.30, 0.70, 0.34]
            };
            (top, [0.33, 0.25, 0.16], [0.16, 0.22, 0.14])
        }
        Block::Dirt => ([0.40, 0.28, 0.18], [0.35, 0.23, 0.14], [0.22, 0.14, 0.09]),
        Block::Stone => ([0.55, 0.57, 0.60], [0.48, 0.50, 0.53], [0.35, 0.36, 0.39]),
        Block::Air => ([0.0; 3], [0.0; 3], [0.0; 3]),
    }
}
