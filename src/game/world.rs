use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use glam::Vec3;

use crate::mesh::{Vertex, push_quad};

pub const CHUNK_SIZE: i64 = 16;
pub const WORLD_HEIGHT: i32 = 64;
pub const DEFAULT_WORLD_SEED: i64 = 0x5EED_BA5E_u64 as i64;

#[repr(u8)]
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

    fn to_id(self) -> u8 {
        self as u8
    }

    fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Grass,
            2 => Self::Dirt,
            3 => Self::Stone,
            _ => Self::Air,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: i64,
    pub y: i32,
    pub z: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainConfig {
    pub base_height: f32,
    pub macro_scale: f32,
    pub macro_amplitude: f32,
    pub detail_scale: f32,
    pub detail_amplitude: f32,
    pub mountain_scale: f32,
    pub mountain_amplitude: f32,
    pub valley_scale: f32,
    pub valley_depth: f32,
    pub cliff_scale: f32,
    pub cliff_strength: f32,
    pub terrace_step: f32,
}

impl TerrainConfig {
    pub fn balanced() -> Self {
        Self {
            base_height: (WORLD_HEIGHT as f32) * 0.50,
            macro_scale: 0.012,
            macro_amplitude: 8.0,
            detail_scale: 0.045,
            detail_amplitude: 2.0,
            mountain_scale: 0.009,
            mountain_amplitude: 10.0,
            valley_scale: 0.010,
            valley_depth: 6.0,
            cliff_scale: 0.020,
            cliff_strength: 0.30,
            terrace_step: 2.5,
        }
    }

    pub fn alpine() -> Self {
        Self {
            mountain_amplitude: 18.0,
            valley_depth: 4.0,
            cliff_strength: 0.42,
            ..Self::balanced()
        }
    }

    pub fn canyon() -> Self {
        Self {
            base_height: (WORLD_HEIGHT as f32) * 0.44,
            macro_amplitude: 6.0,
            mountain_amplitude: 5.0,
            valley_depth: 12.0,
            cliff_strength: 0.62,
            terrace_step: 3.5,
            ..Self::balanced()
        }
    }

    pub fn valleylands() -> Self {
        Self {
            macro_amplitude: 7.0,
            mountain_amplitude: 6.0,
            valley_depth: 9.0,
            cliff_strength: 0.24,
            ..Self::balanced()
        }
    }

    pub fn from_profile_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "balanced" => Some(Self::balanced()),
            "alpine" | "mountain" | "mountains" => Some(Self::alpine()),
            "canyon" | "cliff" | "cliffs" => Some(Self::canyon()),
            "valleylands" | "valley" | "valleys" => Some(Self::valleylands()),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct World {
    overrides: Arc<RwLock<HashMap<BlockPos, Block>>>,
    spawn_point: Vec3,
    seed: i64,
    terrain: TerrainConfig,
}

impl World {
    pub fn generate(_size_x: i32, _size_y: i32, _size_z: i32) -> Self {
        Self::generate_with_terrain(TerrainConfig::balanced())
    }

    pub fn generate_with_terrain(terrain: TerrainConfig) -> Self {
        Self::generate_with_terrain_and_seed(terrain, DEFAULT_WORLD_SEED)
    }

    pub fn generate_with_terrain_and_seed(terrain: TerrainConfig, seed: i64) -> Self {
        let mut world = Self {
            overrides: Arc::new(RwLock::new(HashMap::new())),
            spawn_point: Vec3::ZERO,
            seed,
            terrain,
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

    pub fn terrain_seed(&self) -> i64 {
        self.seed
    }

    pub fn terrain_config(&self) -> TerrainConfig {
        self.terrain
    }

    pub fn build_chunk_mesh(&self, chunk: (i64, i64)) -> Vec<Vertex> {
        let overrides = self.overrides.read().expect("overrides read lock poisoned");
        self.build_chunk_mesh_with_overrides(chunk, &overrides)
    }

    pub fn build_chunk_mesh_lod(&self, origin_chunk: (i64, i64), lod_level: u8) -> Vec<Vertex> {
        if lod_level == 0 {
            return self.build_chunk_mesh(origin_chunk);
        }

        let chunk_span = 1_i64 << lod_level;
        let tile_size_blocks = CHUNK_SIZE * chunk_span;
        let sample_blocks = 2_i64 << lod_level;
        let cells = tile_size_blocks / sample_blocks;
        let base_x = origin_chunk.0 * CHUNK_SIZE;
        let base_z = origin_chunk.1 * CHUNK_SIZE;
        let mut vertices = Vec::new();

        let mut sampled_heights = vec![0.0_f32; ((cells + 1) * (cells + 1)) as usize];
        let height_at =
            |heights: &[f32], x: i64, z: i64| -> f32 { heights[(z * (cells + 1) + x) as usize] };
        let set_height = |heights: &mut [f32], x: i64, z: i64, value: f32| {
            let index = (z * (cells + 1) + x) as usize;
            heights[index] = value;
        };

        for gz in 0..=cells {
            for gx in 0..=cells {
                let sample_x = base_x + gx * sample_blocks + sample_blocks / 2;
                let sample_z = base_z + gz * sample_blocks + sample_blocks / 2;
                let top_y = self.surface_height(sample_x, sample_z) as f32 + 1.0;
                set_height(&mut sampled_heights, gx, gz, top_y);
            }
        }

        for gz in 0..cells {
            for gx in 0..cells {
                let wx0 = base_x + gx * sample_blocks;
                let wz0 = base_z + gz * sample_blocks;
                let wx1 = wx0 + sample_blocks;
                let wz1 = wz0 + sample_blocks;

                let center_x = wx0 + sample_blocks / 2;
                let center_z = wz0 + sample_blocks / 2;
                let top_y = height_at(&sampled_heights, gx, gz);
                let east_y = height_at(&sampled_heights, gx + 1, gz);
                let south_y = height_at(&sampled_heights, gx, gz + 1);
                let block = self.procedural_block(center_x, top_y as i32 - 1, center_z);
                let (top_color, side_color, _) = palette(block, center_x, center_z);

                push_quad(
                    &mut vertices,
                    [
                        Vec3::new(wx0 as f32, top_y, wz0 as f32),
                        Vec3::new(wx0 as f32, top_y, wz1 as f32),
                        Vec3::new(wx1 as f32, top_y, wz1 as f32),
                        Vec3::new(wx1 as f32, top_y, wz0 as f32),
                    ],
                    top_color,
                    Vec3::Y,
                );

                if (top_y - east_y).abs() > 0.01 {
                    let low = top_y.min(east_y);
                    let high = top_y.max(east_y);
                    if top_y > east_y {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, low, wz0 as f32),
                                Vec3::new(wx1 as f32, high, wz0 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                            ],
                            side_color,
                            Vec3::X,
                        );
                    } else {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx1 as f32, low, wz0 as f32),
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz0 as f32),
                            ],
                            side_color,
                            -Vec3::X,
                        );
                    }
                }

                if (top_y - south_y).abs() > 0.01 {
                    let low = top_y.min(south_y);
                    let high = top_y.max(south_y);
                    if top_y > south_y {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx0 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                                Vec3::new(wx0 as f32, high, wz1 as f32),
                            ],
                            side_color,
                            Vec3::Z,
                        );
                    } else {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx0 as f32, low, wz1 as f32),
                                Vec3::new(wx0 as f32, high, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                            ],
                            side_color,
                            -Vec3::Z,
                        );
                    }
                }
            }
        }

        vertices
    }

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
        let base_x = chunk.0 * CHUNK_SIZE;
        let base_z = chunk.1 * CHUNK_SIZE;
        let xz_extent = (CHUNK_SIZE + 2) as usize;
        let y_extent = WORLD_HEIGHT as usize;
        let layer_stride = xz_extent * xz_extent;
        let mut block_ids = vec![Block::Air.to_id(); layer_stride * y_extent];
        let mut column_top = vec![0_usize; xz_extent * xz_extent];

        let grid_index = |x: usize, y: usize, z: usize| y * layer_stride + z * xz_extent + x;
        let column_index = |x: usize, z: usize| z * xz_extent + x;

        // Populate a chunk-local voxel cache (with a 1-block border) once, then mesh from it.
        for local_z in 0..xz_extent {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 0..xz_extent {
                let world_x = base_x + local_x as i64 - 1;
                let surface_y = self.surface_height(world_x, world_z);
                let clamped_top = surface_y.clamp(0, WORLD_HEIGHT - 1) as usize;
                column_top[column_index(local_x, local_z)] = clamped_top;
                for y in 0..=clamped_top {
                    let block = procedural_block_for_height(surface_y, y as i32);
                    block_ids[grid_index(local_x, y, local_z)] = block.to_id();
                }
            }
        }

        if !overrides.is_empty() {
            let min_x = base_x - 1;
            let max_x = base_x + CHUNK_SIZE;
            let min_z = base_z - 1;
            let max_z = base_z + CHUNK_SIZE;
            for (pos, block) in overrides {
                if !(0..WORLD_HEIGHT).contains(&pos.y) {
                    continue;
                }
                if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
                    continue;
                }
                let local_x = (pos.x - base_x + 1) as usize;
                let local_z = (pos.z - base_z + 1) as usize;
                let y = pos.y as usize;
                block_ids[grid_index(local_x, y, local_z)] = block.to_id();
                if block.is_solid() {
                    let top = &mut column_top[column_index(local_x, local_z)];
                    if y > *top {
                        *top = y;
                    }
                }
            }
        }

        let mut vertices = Vec::with_capacity(24_576);
        for local_z in 1..=CHUNK_SIZE as usize {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 1..=CHUNK_SIZE as usize {
                let world_x = base_x + local_x as i64 - 1;
                let top =
                    column_top[column_index(local_x, local_z)].min(y_extent.saturating_sub(1));
                for y in 0..=top {
                    let block_id = block_ids[grid_index(local_x, y, local_z)];
                    if block_id == Block::Air.to_id() {
                        continue;
                    }
                    let block = Block::from_id(block_id);
                    let base = Vec3::new(world_x as f32, y as f32, world_z as f32);
                    let (top_color, side_color, bottom_color) = palette(block, world_x, world_z);

                    if y + 1 >= y_extent
                        || block_ids[grid_index(local_x, y + 1, local_z)] == Block::Air.to_id()
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
                            Vec3::Y,
                        );
                    }
                    if y > 0 && block_ids[grid_index(local_x, y - 1, local_z)] == Block::Air.to_id()
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
                            -Vec3::Y,
                        );
                    }
                    if block_ids[grid_index(local_x - 1, y, local_z)] == Block::Air.to_id() {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                            ],
                            side_color,
                            -Vec3::X,
                        );
                    }
                    if block_ids[grid_index(local_x + 1, y, local_z)] == Block::Air.to_id() {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                            ],
                            side_color,
                            Vec3::X,
                        );
                    }
                    if block_ids[grid_index(local_x, y, local_z - 1)] == Block::Air.to_id() {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            side_color,
                            -Vec3::Z,
                        );
                    }
                    if block_ids[grid_index(local_x, y, local_z + 1)] == Block::Air.to_id() {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                            ],
                            side_color,
                            Vec3::Z,
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
        procedural_block_for_height(surface_y, y)
    }

    fn surface_height(&self, x: i64, z: i64) -> i32 {
        let cfg = self.terrain;
        let fx = x as f32;
        let fz = z as f32;

        let macro_noise = fbm_2d(
            self.seed,
            fx * cfg.macro_scale,
            fz * cfg.macro_scale,
            4,
            2.0,
            0.5,
        );
        let detail_noise = fbm_2d(
            self.seed.wrapping_add(0x6A09_E667),
            fx * cfg.detail_scale,
            fz * cfg.detail_scale,
            3,
            2.0,
            0.55,
        );
        let mountain_noise = ridge_fbm_2d(
            self.seed.wrapping_add(0xBB67_AE85),
            fx * cfg.mountain_scale,
            fz * cfg.mountain_scale,
            4,
            2.0,
            0.48,
        );
        let valley_noise = fbm_2d(
            self.seed.wrapping_add(0x3C6E_F372),
            fx * cfg.valley_scale,
            fz * cfg.valley_scale,
            3,
            2.0,
            0.50,
        )
        .max(0.0)
        .powf(1.35);

        let mut height = cfg.base_height
            + macro_noise * cfg.macro_amplitude
            + detail_noise * cfg.detail_amplitude
            + mountain_noise * cfg.mountain_amplitude
            - valley_noise * cfg.valley_depth;

        if cfg.cliff_strength > 0.0 {
            let cliff_mask = fbm_2d(
                self.seed.wrapping_add(0xA54F_F53A),
                fx * cfg.cliff_scale,
                fz * cfg.cliff_scale,
                2,
                2.0,
                0.5,
            )
            .abs()
            .powf(1.4);
            let terraced = (height / cfg.terrace_step).round() * cfg.terrace_step;
            let blend = (cliff_mask * cfg.cliff_strength).clamp(0.0, 1.0);
            height = height * (1.0 - blend) + terraced * blend;
        }

        (height.round() as i32).clamp(2, WORLD_HEIGHT - 4)
    }
}

fn ridge_fbm_2d(seed: i64, x: f32, z: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;

    for octave in 0..octaves {
        let n = value_noise_2d(
            seed.wrapping_add(octave as i64 * 97),
            x * frequency,
            z * frequency,
        );
        let ridge = 1.0 - n.abs();
        sum += ridge * amplitude;
        norm += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    if norm <= f32::EPSILON {
        0.0
    } else {
        (sum / norm) * 2.0 - 1.0
    }
}

fn fbm_2d(seed: i64, x: f32, z: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;

    for octave in 0..octaves {
        let n = value_noise_2d(
            seed.wrapping_add(octave as i64 * 131),
            x * frequency,
            z * frequency,
        );
        sum += n * amplitude;
        norm += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    if norm <= f32::EPSILON {
        0.0
    } else {
        sum / norm
    }
}

fn value_noise_2d(seed: i64, x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i64;
    let z0 = z.floor() as i64;
    let x1 = x0 + 1;
    let z1 = z0 + 1;
    let tx = smoothstep(x - x.floor());
    let tz = smoothstep(z - z.floor());

    let v00 = hash2_to_unit(seed, x0, z0);
    let v10 = hash2_to_unit(seed, x1, z0);
    let v01 = hash2_to_unit(seed, x0, z1);
    let v11 = hash2_to_unit(seed, x1, z1);

    let a = lerp(v00, v10, tx);
    let b = lerp(v01, v11, tx);
    lerp(a, b, tz)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash2_to_unit(seed: i64, x: i64, z: i64) -> f32 {
    let mut h = seed as u64;
    h ^= (x as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    h ^= (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;

    let u = (h as f64) / (u64::MAX as f64);
    (u as f32) * 2.0 - 1.0
}

fn div_floor(value: i64, divisor: i64) -> i64 {
    let mut result = value / divisor;
    let remainder = value % divisor;
    if remainder != 0 && (remainder > 0) != (divisor > 0) {
        result -= 1;
    }
    result
}

fn procedural_block_for_height(surface_y: i32, y: i32) -> Block {
    if y == surface_y {
        return Block::Grass;
    }
    if y >= surface_y - 2 {
        return Block::Dirt;
    }
    Block::Stone
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_stays_within_world_bounds() {
        let world = World::generate(64, 32, 64);
        for z in -128..=128 {
            for x in -128..=128 {
                let y = world.surface_height(x, z);
                assert!((2..=(WORLD_HEIGHT - 4)).contains(&y));
            }
        }
    }

    #[test]
    fn terrain_presets_produce_different_profiles() {
        let balanced = World::generate_with_terrain(TerrainConfig::balanced());
        let alpine = World::generate_with_terrain(TerrainConfig::alpine());
        let canyon = World::generate_with_terrain(TerrainConfig::canyon());

        let point = (96, -48);
        let a = balanced.surface_height(point.0, point.1);
        let b = alpine.surface_height(point.0, point.1);
        let c = canyon.surface_height(point.0, point.1);

        assert!(a != b || b != c || a != c);
    }
}
