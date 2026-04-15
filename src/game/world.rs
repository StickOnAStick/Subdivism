use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use glam::Vec3;

use crate::mesh::{Vertex, push_quad};

pub const CHUNK_SIZE: i64 = 16;
pub const WORLD_MIN_Y: i32 = -256;
pub const WORLD_MAX_Y: i32 = 1024;
pub const WORLD_HEIGHT: i32 = WORLD_MAX_Y - WORLD_MIN_Y + 1;
pub const WORLD_OVERWORLD_FLOOR: i32 = 255;
pub const DEFAULT_WORLD_SEED: i64 = 0x5EED_BA5E_u64 as i64;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
    Deepslate,
    DeepDark,
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
            4 => Self::Deepslate,
            5 => Self::DeepDark,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubBlockPos {
    pub x: i64,
    pub y: i32,
    pub z: i64,
    pub divisions: u8,
    pub sx: u8,
    pub sy: u8,
    pub sz: u8,
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
    pub biome_scale: f32,
    pub biome_blend: f32,
    pub mountain_base_lift: f32,
    pub desert_base_drop: f32,
    pub desert_dune_scale: f32,
    pub desert_dune_amplitude: f32,
    pub ravine_scale: f32,
    pub ravine_strength: f32,
    pub ravine_width: f32,
    pub ravine_offset_x: f32,
    pub ravine_offset_z: f32,
}

impl TerrainConfig {
    pub fn parameter_keys() -> &'static [&'static str] {
        &[
            "base_height",
            "macro_scale",
            "macro_amplitude",
            "detail_scale",
            "detail_amplitude",
            "mountain_scale",
            "mountain_amplitude",
            "valley_scale",
            "valley_depth",
            "cliff_scale",
            "cliff_strength",
            "terrace_step",
            "biome_scale",
            "biome_blend",
            "mountain_base_lift",
            "desert_base_drop",
            "desert_dune_scale",
            "desert_dune_amplitude",
            "ravine_scale",
            "ravine_strength",
            "ravine_width",
            "ravine_offset_x",
            "ravine_offset_z",
        ]
    }

    pub fn balanced() -> Self {
        Self {
            base_height: 340.0,
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
            biome_scale: 0.0038,
            biome_blend: 0.14,
            mountain_base_lift: 5.0,
            desert_base_drop: 5.0,
            desert_dune_scale: 0.030,
            desert_dune_amplitude: 2.2,
            ravine_scale: 0.012,
            ravine_strength: 2.0,
            ravine_width: 4.5,
            ravine_offset_x: 0.0,
            ravine_offset_z: 0.0,
        }
    }

    pub fn alpine() -> Self {
        Self {
            mountain_amplitude: 18.0,
            valley_depth: 4.0,
            cliff_strength: 0.42,
            mountain_base_lift: 10.0,
            desert_base_drop: 3.0,
            ravine_strength: 2.6,
            ..Self::balanced()
        }
    }

    pub fn canyon() -> Self {
        Self {
            base_height: 316.0,
            macro_amplitude: 6.0,
            mountain_amplitude: 5.0,
            valley_depth: 12.0,
            cliff_strength: 0.62,
            terrace_step: 3.5,
            desert_base_drop: 9.0,
            desert_dune_amplitude: 3.6,
            ravine_strength: 3.4,
            ravine_width: 5.2,
            ..Self::balanced()
        }
    }

    pub fn valleylands() -> Self {
        Self {
            macro_amplitude: 7.0,
            mountain_amplitude: 6.0,
            valley_depth: 9.0,
            cliff_strength: 0.24,
            biome_blend: 0.18,
            ravine_strength: 1.5,
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

    pub fn set_named_param(&mut self, key: &str, value: f32) -> Result<(), String> {
        match key.to_ascii_lowercase().as_str() {
            "base_height" => self.base_height = value,
            "macro_scale" => self.macro_scale = value,
            "macro_amplitude" => self.macro_amplitude = value,
            "detail_scale" => self.detail_scale = value,
            "detail_amplitude" => self.detail_amplitude = value,
            "mountain_scale" => self.mountain_scale = value,
            "mountain_amplitude" => self.mountain_amplitude = value,
            "valley_scale" => self.valley_scale = value,
            "valley_depth" => self.valley_depth = value,
            "cliff_scale" => self.cliff_scale = value,
            "cliff_strength" => self.cliff_strength = value,
            "terrace_step" => self.terrace_step = value,
            "biome_scale" => self.biome_scale = value,
            "biome_blend" => self.biome_blend = value,
            "mountain_base_lift" => self.mountain_base_lift = value,
            "desert_base_drop" => self.desert_base_drop = value,
            "desert_dune_scale" => self.desert_dune_scale = value,
            "desert_dune_amplitude" => self.desert_dune_amplitude = value,
            "ravine_scale" => self.ravine_scale = value,
            "ravine_strength" => self.ravine_strength = value,
            "ravine_width" => self.ravine_width = value,
            "ravine_offset_x" => self.ravine_offset_x = value,
            "ravine_offset_z" => self.ravine_offset_z = value,
            _ => return Err(format!("unknown terrain parameter `{key}`")),
        }
        self.clamp_reasonable();
        Ok(())
    }

    pub fn clamp_reasonable(&mut self) {
        self.base_height = self
            .base_height
            .clamp((WORLD_OVERWORLD_FLOOR + 8) as f32, (WORLD_MAX_Y - 8) as f32);
        self.macro_scale = self.macro_scale.clamp(0.0015, 0.08);
        self.macro_amplitude = self.macro_amplitude.clamp(0.0, 30.0);
        self.detail_scale = self.detail_scale.clamp(0.005, 0.30);
        self.detail_amplitude = self.detail_amplitude.clamp(0.0, 12.0);
        self.mountain_scale = self.mountain_scale.clamp(0.0015, 0.08);
        self.mountain_amplitude = self.mountain_amplitude.clamp(0.0, 40.0);
        self.valley_scale = self.valley_scale.clamp(0.0015, 0.08);
        self.valley_depth = self.valley_depth.clamp(0.0, 24.0);
        self.cliff_scale = self.cliff_scale.clamp(0.004, 0.16);
        self.cliff_strength = self.cliff_strength.clamp(0.0, 1.0);
        self.terrace_step = self.terrace_step.clamp(0.4, 10.0);
        self.biome_scale = self.biome_scale.clamp(0.0008, 0.03);
        self.biome_blend = self.biome_blend.clamp(0.02, 0.45);
        self.mountain_base_lift = self.mountain_base_lift.clamp(-8.0, 20.0);
        self.desert_base_drop = self.desert_base_drop.clamp(0.0, 20.0);
        self.desert_dune_scale = self.desert_dune_scale.clamp(0.005, 0.20);
        self.desert_dune_amplitude = self.desert_dune_amplitude.clamp(0.0, 12.0);
        self.ravine_scale = self.ravine_scale.clamp(0.001, 0.08);
        self.ravine_strength = self.ravine_strength.clamp(0.0, 16.0);
        self.ravine_width = self.ravine_width.clamp(1.0, 16.0);
        self.ravine_offset_x = self.ravine_offset_x.clamp(-4096.0, 4096.0);
        self.ravine_offset_z = self.ravine_offset_z.clamp(-4096.0, 4096.0);
    }
}

#[derive(Clone)]
pub struct World {
    overrides: Arc<RwLock<HashMap<BlockPos, Block>>>,
    sub_overrides: Arc<RwLock<HashMap<SubBlockPos, Block>>>,
    spawn_point: Vec3,
    seed: i64,
    terrain: TerrainConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BiomeKind {
    Ocean,
    Meadow,
    Valley,
    Mountain,
    Desert,
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
            sub_overrides: Arc::new(RwLock::new(HashMap::new())),
            spawn_point: Vec3::ZERO,
            seed,
            terrain,
        };
        world.spawn_point = world
            .spawn_point_for_column(0, 0)
            .unwrap_or(Vec3::new(0.5, (WORLD_OVERWORLD_FLOOR + 3) as f32, 0.5));
        world
    }

    pub fn spawn_point(&self) -> Vec3 {
        self.spawn_point
    }

    pub fn is_out_of_bounds(&self, position: Vec3, margin: f32) -> bool {
        position.y < WORLD_MIN_Y as f32 - margin
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
                let (top_color, side_color, _) = self.palette_at(block, center_x, center_z);

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

        let boundary_height = |sample_x: i64, sample_z: i64| -> f32 {
            self.surface_height(sample_x, sample_z) as f32 + 1.0
        };

        // Close seams on tile borders so differing neighbor heights do not expose void slits.
        for gz in 0..cells {
            let wz0 = base_z + gz * sample_blocks;
            let wz1 = wz0 + sample_blocks;
            let sample_z = wz0 + sample_blocks / 2;

            let west_inside = height_at(&sampled_heights, 0, gz);
            let west_outside = boundary_height(base_x - sample_blocks / 2, sample_z);
            let center_z = wz0 + sample_blocks / 2;
            let west_block = self.procedural_block(base_x, west_inside as i32 - 1, center_z);
            let (_, west_side_color, _) = self.palette_at(west_block, base_x, center_z);
            if (west_inside - west_outside).abs() > 0.01 {
                let low = west_inside.min(west_outside);
                let high = west_inside.max(west_outside);
                if west_inside > west_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(base_x as f32, low, wz0 as f32),
                            Vec3::new(base_x as f32, low, wz1 as f32),
                            Vec3::new(base_x as f32, high, wz1 as f32),
                            Vec3::new(base_x as f32, high, wz0 as f32),
                        ],
                        west_side_color,
                        -Vec3::X,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(base_x as f32, low, wz1 as f32),
                            Vec3::new(base_x as f32, low, wz0 as f32),
                            Vec3::new(base_x as f32, high, wz0 as f32),
                            Vec3::new(base_x as f32, high, wz1 as f32),
                        ],
                        west_side_color,
                        Vec3::X,
                    );
                }
            }

            let east_x = base_x + tile_size_blocks;
            let east_inside = height_at(&sampled_heights, cells - 1, gz);
            let east_outside = boundary_height(east_x + sample_blocks / 2, sample_z);
            let east_block = self.procedural_block(east_x - 1, east_inside as i32 - 1, center_z);
            let (_, east_side_color, _) = self.palette_at(east_block, east_x - 1, center_z);
            if (east_inside - east_outside).abs() > 0.01 {
                let low = east_inside.min(east_outside);
                let high = east_inside.max(east_outside);
                if east_inside > east_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(east_x as f32, low, wz1 as f32),
                            Vec3::new(east_x as f32, low, wz0 as f32),
                            Vec3::new(east_x as f32, high, wz0 as f32),
                            Vec3::new(east_x as f32, high, wz1 as f32),
                        ],
                        east_side_color,
                        Vec3::X,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(east_x as f32, low, wz0 as f32),
                            Vec3::new(east_x as f32, low, wz1 as f32),
                            Vec3::new(east_x as f32, high, wz1 as f32),
                            Vec3::new(east_x as f32, high, wz0 as f32),
                        ],
                        east_side_color,
                        -Vec3::X,
                    );
                }
            }
        }

        for gx in 0..cells {
            let wx0 = base_x + gx * sample_blocks;
            let wx1 = wx0 + sample_blocks;
            let sample_x = wx0 + sample_blocks / 2;

            let north_inside = height_at(&sampled_heights, gx, 0);
            let north_outside = boundary_height(sample_x, base_z - sample_blocks / 2);
            let center_x = wx0 + sample_blocks / 2;
            let north_block = self.procedural_block(center_x, north_inside as i32 - 1, base_z);
            let (_, north_side_color, _) = self.palette_at(north_block, center_x, base_z);
            if (north_inside - north_outside).abs() > 0.01 {
                let low = north_inside.min(north_outside);
                let high = north_inside.max(north_outside);
                if north_inside > north_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx1 as f32, low, base_z as f32),
                            Vec3::new(wx0 as f32, low, base_z as f32),
                            Vec3::new(wx0 as f32, high, base_z as f32),
                            Vec3::new(wx1 as f32, high, base_z as f32),
                        ],
                        north_side_color,
                        -Vec3::Z,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx0 as f32, low, base_z as f32),
                            Vec3::new(wx1 as f32, low, base_z as f32),
                            Vec3::new(wx1 as f32, high, base_z as f32),
                            Vec3::new(wx0 as f32, high, base_z as f32),
                        ],
                        north_side_color,
                        Vec3::Z,
                    );
                }
            }

            let south_z = base_z + tile_size_blocks;
            let south_inside = height_at(&sampled_heights, gx, cells - 1);
            let south_outside = boundary_height(sample_x, south_z + sample_blocks / 2);
            let south_block = self.procedural_block(center_x, south_inside as i32 - 1, south_z - 1);
            let (_, south_side_color, _) = self.palette_at(south_block, center_x, south_z - 1);
            if (south_inside - south_outside).abs() > 0.01 {
                let low = south_inside.min(south_outside);
                let high = south_inside.max(south_outside);
                if south_inside > south_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx0 as f32, low, south_z as f32),
                            Vec3::new(wx1 as f32, low, south_z as f32),
                            Vec3::new(wx1 as f32, high, south_z as f32),
                            Vec3::new(wx0 as f32, high, south_z as f32),
                        ],
                        south_side_color,
                        Vec3::Z,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx1 as f32, low, south_z as f32),
                            Vec3::new(wx0 as f32, low, south_z as f32),
                            Vec3::new(wx0 as f32, high, south_z as f32),
                            Vec3::new(wx1 as f32, high, south_z as f32),
                        ],
                        south_side_color,
                        -Vec3::Z,
                    );
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
        if !contains_world_y(y) {
            return;
        }
        self.overrides
            .write()
            .expect("overrides write lock poisoned")
            .insert(BlockPos { x, y, z }, block);
        self.sub_overrides
            .write()
            .expect("sub overrides write lock poisoned")
            .retain(|pos, _| !(pos.x == x && pos.y == y && pos.z == z));
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
        self.sub_overrides
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
        self.sub_overrides
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
        self.sub_overrides
            .read()
            .expect("sub overrides read lock poisoned")
            .get(&pos)
            .copied()
    }

    pub fn sub_blocks_in_cell(&self, x: i64, y: i32, z: i64) -> Vec<(SubBlockPos, Block)> {
        if !contains_world_y(y) {
            return Vec::new();
        }
        self.sub_overrides
            .read()
            .expect("sub overrides read lock poisoned")
            .iter()
            .filter_map(|(pos, block)| {
                if pos.x == x && pos.y == y && pos.z == z {
                    Some((*pos, *block))
                } else {
                    None
                }
            })
            .collect()
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
        let mut column_top = vec![WORLD_MIN_Y; xz_extent * xz_extent];
        let min_x = base_x - 1;
        let max_x = base_x + CHUNK_SIZE;
        let min_z = base_z - 1;
        let max_z = base_z + CHUNK_SIZE;
        let mut min_fill_y = WORLD_OVERWORLD_FLOOR - 32;

        if !overrides.is_empty() {
            for (pos, _) in overrides {
                if !contains_world_y(pos.y) {
                    continue;
                }
                if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
                    continue;
                }
                if pos.y < min_fill_y {
                    min_fill_y = (pos.y - 1).max(WORLD_MIN_Y);
                }
            }
        }

        let grid_index = |x: usize, y: usize, z: usize| y * layer_stride + z * xz_extent + x;
        let column_index = |x: usize, z: usize| z * xz_extent + x;

        // Populate a chunk-local voxel cache (with a 1-block border) once, then mesh from it.
        for local_z in 0..xz_extent {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 0..xz_extent {
                let world_x = base_x + local_x as i64 - 1;
                let surface_y = self.surface_height(world_x, world_z);
                let clamped_top = surface_y.clamp(WORLD_MIN_Y, WORLD_MAX_Y);
                column_top[column_index(local_x, local_z)] = clamped_top;
                for y in min_fill_y..=clamped_top {
                    let Some(yi) = world_y_to_index(y) else {
                        continue;
                    };
                    let block = self.procedural_block_with_surface(world_x, y, world_z, surface_y);
                    block_ids[grid_index(local_x, yi, local_z)] = block.to_id();
                }
            }
        }

        if !overrides.is_empty() {
            for (pos, block) in overrides {
                if !contains_world_y(pos.y) {
                    continue;
                }
                if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
                    continue;
                }
                let local_x = (pos.x - base_x + 1) as usize;
                let local_z = (pos.z - base_z + 1) as usize;
                let Some(y) = world_y_to_index(pos.y) else {
                    continue;
                };
                block_ids[grid_index(local_x, y, local_z)] = block.to_id();
                if block.is_solid() {
                    let top = &mut column_top[column_index(local_x, local_z)];
                    if pos.y > *top {
                        *top = pos.y;
                    }
                }
            }
        }

        let light_levels = build_light_volume(&block_ids, xz_extent, y_extent);

        let mut vertices = Vec::with_capacity(24_576);
        for local_z in 1..=CHUNK_SIZE as usize {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 1..=CHUNK_SIZE as usize {
                let world_x = base_x + local_x as i64 - 1;
                let top = column_top[column_index(local_x, local_z)];
                for y in min_fill_y..=top {
                    let Some(yi) = world_y_to_index(y) else {
                        continue;
                    };
                    let block_id = block_ids[grid_index(local_x, yi, local_z)];
                    if block_id == Block::Air.to_id() {
                        continue;
                    }
                    let block = Block::from_id(block_id);
                    let base = Vec3::new(world_x as f32, y as f32, world_z as f32);
                    let (top_color, side_color, bottom_color) =
                        self.palette_at(block, world_x, world_z);

                    if yi + 1 >= y_extent
                        || block_ids[grid_index(local_x, yi + 1, local_z)] == Block::Air.to_id()
                    {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi + 1,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            apply_light(top_color, light, 1.0),
                            Vec3::Y,
                        );
                    }
                    if yi > 0
                        && block_ids[grid_index(local_x, yi - 1, local_z)] == Block::Air.to_id()
                    {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi - 1,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                            ],
                            apply_light(bottom_color, light, 0.62),
                            -Vec3::Y,
                        );
                    }
                    if block_ids[grid_index(local_x - 1, yi, local_z)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x - 1,
                            yi,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                            ],
                            apply_light(side_color, light, 0.78),
                            -Vec3::X,
                        );
                    }
                    if block_ids[grid_index(local_x + 1, yi, local_z)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x + 1,
                            yi,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                            ],
                            apply_light(side_color, light, 0.84),
                            Vec3::X,
                        );
                    }
                    if block_ids[grid_index(local_x, yi, local_z - 1)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi,
                            local_z - 1,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            apply_light(side_color, light, 0.76),
                            -Vec3::Z,
                        );
                    }
                    if block_ids[grid_index(local_x, yi, local_z + 1)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi,
                            local_z + 1,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                            ],
                            apply_light(side_color, light, 0.82),
                            Vec3::Z,
                        );
                    }
                }
            }
        }

        let sub_overrides = self
            .sub_overrides
            .read()
            .expect("sub overrides read lock poisoned");
        let chunk_max_x = base_x + CHUNK_SIZE - 1;
        let chunk_max_z = base_z + CHUNK_SIZE - 1;
        for (pos, block) in sub_overrides.iter() {
            if !contains_world_y(pos.y)
                || pos.x < base_x
                || pos.x > chunk_max_x
                || pos.z < base_z
                || pos.z > chunk_max_z
                || !block.is_solid()
            {
                continue;
            }
            add_sub_block_cube(&mut vertices, *pos, *block, self);
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
        if !contains_world_y(y) {
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
        self.procedural_block_with_surface(x, y, z, surface_y)
    }

    fn procedural_block_with_surface(&self, x: i64, y: i32, z: i64, surface_y: i32) -> Block {
        if y > surface_y {
            return Block::Air;
        }

        // Carve cave pockets mostly through stone layers beneath the overworld.
        if y < surface_y - 4
            && ((WORLD_OVERWORLD_FLOOR - 32)..=WORLD_OVERWORLD_FLOOR).contains(&y)
            && should_carve_cave(self.seed, x, y, z)
        {
            return Block::Air;
        }

        if y == surface_y && y >= WORLD_OVERWORLD_FLOOR {
            return Block::Grass;
        }
        if y >= surface_y - 2 && y >= WORLD_OVERWORLD_FLOOR - 16 {
            return Block::Dirt;
        }
        crust_block_for_y(y)
    }

    fn surface_height(&self, x: i64, z: i64) -> i32 {
        let cfg = self.terrain;
        let fx = x as f32;
        let fz = z as f32;

        let biome_warp = fbm_2d(
            self.seed.wrapping_add(0x9E37_79B9),
            fx * cfg.biome_scale * 0.28,
            fz * cfg.biome_scale * 0.28,
            3,
            2.0,
            0.55,
        ) * 28.0;
        let climate_noise = fbm_2d(
            self.seed.wrapping_add(0x7F4A_7C15),
            (fx + biome_warp) * cfg.biome_scale,
            (fz - biome_warp * 0.6) * cfg.biome_scale,
            4,
            2.0,
            0.5,
        );
        let continent_noise = fbm_2d(
            self.seed.wrapping_add(0x243F_6A88),
            (fx + biome_warp * 0.25) * cfg.biome_scale * 0.62,
            (fz - biome_warp * 0.20) * cfg.biome_scale * 0.62,
            4,
            2.0,
            0.50,
        );
        let biome_blend = cfg.biome_blend.max(0.02);
        let ocean_mask = 1.0
            - smooth_range(
                continent_noise,
                -0.30 - biome_blend * 0.5,
                -0.30 + biome_blend * 0.5,
            );
        let mountain_pref = smooth_range(
            continent_noise,
            0.20 - biome_blend * 0.7,
            0.20 + biome_blend * 0.7,
        );
        let desert_pref = smooth_range(climate_noise, 0.18 - biome_blend, 0.18 + biome_blend)
            * (1.0 - ocean_mask * 0.85);

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
        let valley_pref = smooth_range(
            valley_noise,
            0.32 - biome_blend * 0.4,
            0.70 + biome_blend * 0.4,
        ) * (1.0 - mountain_pref * 0.7)
            * (1.0 - ocean_mask * 0.9);
        let desert_dune_noise = fbm_2d(
            self.seed.wrapping_add(0x1F83_D9AB),
            fx * cfg.desert_dune_scale,
            fz * cfg.desert_dune_scale,
            4,
            2.15,
            0.52,
        );

        let mountain_mask = mountain_pref.max(0.0) * (1.0 - ocean_mask * 0.95);
        let desert_mask = desert_pref.max(0.0) * (1.0 - ocean_mask * 0.9);
        let valley_mask = valley_pref.max(0.0);
        let meadow_mask =
            (1.0 - ocean_mask - mountain_mask - desert_mask - valley_mask).clamp(0.0, 1.0);
        let mask_norm = (ocean_mask + mountain_mask + desert_mask + valley_mask + meadow_mask)
            .max(f32::EPSILON);
        let ocean_mask = ocean_mask / mask_norm;
        let mountain_mask = mountain_mask / mask_norm;
        let desert_mask = desert_mask / mask_norm;
        let valley_mask = valley_mask / mask_norm;
        let meadow_mask = meadow_mask / mask_norm;

        let base_height = cfg.base_height
            + macro_noise * cfg.macro_amplitude
            + detail_noise * cfg.detail_amplitude;
        let ocean_height = (cfg.base_height - cfg.desert_base_drop - 8.0)
            + macro_noise * (cfg.macro_amplitude * 0.28)
            + detail_noise * (cfg.detail_amplitude * 0.22)
            - valley_noise * (cfg.valley_depth * 0.45);
        let mountain_height =
            base_height + mountain_noise * cfg.mountain_amplitude + cfg.mountain_base_lift;
        let meadow_height = base_height + mountain_noise * cfg.mountain_amplitude * 0.35;
        let valley_height = (cfg.base_height - cfg.valley_depth * 0.95)
            + macro_noise * (cfg.macro_amplitude * 0.55)
            + detail_noise * (cfg.detail_amplitude * 0.55)
            + mountain_noise * (cfg.mountain_amplitude * 0.18);
        let desert_height = (cfg.base_height - cfg.desert_base_drop)
            + macro_noise * (cfg.macro_amplitude * 0.42)
            + detail_noise * (cfg.detail_amplitude * 0.30)
            + desert_dune_noise * cfg.desert_dune_amplitude;

        let mut height = ocean_height * ocean_mask
            + mountain_height * mountain_mask
            + meadow_height * meadow_mask
            + valley_height * valley_mask
            + desert_height * desert_mask
            - valley_noise * cfg.valley_depth * (0.72 + 0.28 * meadow_mask);

        let ravine_noise = value_noise_2d(
            self.seed.wrapping_add(0x5BE0_CD19),
            (fx + cfg.ravine_offset_x) * cfg.ravine_scale,
            (fz + cfg.ravine_offset_z) * cfg.ravine_scale,
        );
        let ravine_shape = (1.0 - ravine_noise.abs())
            .max(0.0)
            .powf(cfg.ravine_width.max(1.0));
        let ravine_mask = (0.45 + 0.55 * mountain_mask.max(desert_mask).max(valley_mask))
            * (1.0 - ocean_mask * 0.95);
        height -= ravine_shape * cfg.ravine_strength * ravine_mask;

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

        (height.round() as i32).clamp(WORLD_OVERWORLD_FLOOR, WORLD_MAX_Y - 4)
    }

    pub fn surface_height_at(&self, x: i64, z: i64) -> i32 {
        self.surface_height(x, z)
    }

    fn palette_at(&self, block: Block, x: i64, z: i64) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let biome = self.biome_at(x, z);
        palette(block, x, z, biome)
    }

    fn biome_at(&self, x: i64, z: i64) -> BiomeKind {
        let chunk = Self::world_to_chunk(x, z);
        let primary = self.biome_kind_for_chunk(chunk.0, chunk.1);
        let local_x = x.rem_euclid(CHUNK_SIZE) as f32;
        let local_z = z.rem_euclid(CHUNK_SIZE) as f32;
        let edge_extent = (CHUNK_SIZE - 1) as f32;
        let blend_width = 5.5_f32;

        let mut scores = [0.05_f32; 5];
        scores[biome_index(primary)] += 1.0;
        let mut any_edge_blend = false;

        let neighbors = [
            (
                (chunk.0 - 1, chunk.1),
                local_x,
                z as f32,
                self.seed.wrapping_add(0xA54F_F53A),
            ),
            (
                (chunk.0 + 1, chunk.1),
                edge_extent - local_x,
                z as f32,
                self.seed.wrapping_add(0x510E_527F),
            ),
            (
                (chunk.0, chunk.1 - 1),
                local_z,
                x as f32,
                self.seed.wrapping_add(0x1F83_D9AB),
            ),
            (
                (chunk.0, chunk.1 + 1),
                edge_extent - local_z,
                x as f32,
                self.seed.wrapping_add(0x5BE0_CD19),
            ),
        ];

        for (neighbor_chunk, edge_distance, along_axis, wave_seed) in neighbors {
            let neighbor = self.biome_kind_for_chunk(neighbor_chunk.0, neighbor_chunk.1);
            if neighbor == primary {
                continue;
            }
            let wave = wave_curve(wave_seed, along_axis, x as f32, z as f32);
            let shifted_distance = edge_distance + wave;
            let t = smooth_range(blend_width - shifted_distance, 0.0, blend_width);
            if t <= 0.001 {
                continue;
            }
            any_edge_blend = true;
            scores[biome_index(neighbor)] += t * 0.95;
            scores[biome_index(primary)] += (1.0 - t) * 0.10;
        }

        if !any_edge_blend {
            return primary;
        }

        let total = scores.iter().sum::<f32>().max(f32::EPSILON);
        let mut threshold = ((hash2_to_unit(self.seed ^ 0x94D0_49BB, x, z) + 1.0) * 0.5) * total;
        for (index, score) in scores.iter().enumerate() {
            threshold -= *score;
            if threshold <= 0.0 {
                return biome_from_index(index);
            }
        }
        primary
    }

    fn biome_kind_for_chunk(&self, chunk_x: i64, chunk_z: i64) -> BiomeKind {
        let fx = chunk_x as f32;
        let fz = chunk_z as f32;
        let region = value_noise_2d(
            self.seed ^ 0x6A09E667F3BCC909_u64 as i64,
            fx * 0.095,
            fz * 0.095,
        );
        let moisture = value_noise_2d(
            self.seed ^ 0xBB67AE8584CAA73B_u64 as i64,
            fx * 0.11 + 21.3,
            fz * 0.11 - 37.8,
        );
        let relief = value_noise_2d(
            self.seed ^ 0x3C6EF372FE94F82B_u64 as i64,
            fx * 0.13 - 17.0,
            fz * 0.13 + 9.0,
        );
        let continentality = region * 0.72 + relief * 0.28;
        if continentality < -0.36 {
            BiomeKind::Ocean
        } else if relief > 0.34 || continentality > 0.36 {
            BiomeKind::Mountain
        } else if moisture < -0.24 {
            BiomeKind::Desert
        } else if region < -0.06 && moisture > 0.08 {
            BiomeKind::Valley
        } else {
            BiomeKind::Meadow
        }
    }
}

fn biome_index(kind: BiomeKind) -> usize {
    match kind {
        BiomeKind::Ocean => 0,
        BiomeKind::Meadow => 1,
        BiomeKind::Valley => 2,
        BiomeKind::Mountain => 3,
        BiomeKind::Desert => 4,
    }
}

fn biome_from_index(index: usize) -> BiomeKind {
    match index {
        0 => BiomeKind::Ocean,
        2 => BiomeKind::Valley,
        3 => BiomeKind::Mountain,
        4 => BiomeKind::Desert,
        _ => BiomeKind::Meadow,
    }
}

fn build_light_volume(block_ids: &[u8], xz_extent: usize, y_extent: usize) -> Vec<u8> {
    let layer_stride = xz_extent * xz_extent;
    let grid_index = |x: usize, y: usize, z: usize| y * layer_stride + z * xz_extent + x;
    let mut light_levels = vec![0_u8; block_ids.len()];

    for z in 0..xz_extent {
        for x in 0..xz_extent {
            let mut first_solid_from_top = None;
            for y in (0..y_extent).rev() {
                let idx = grid_index(x, y, z);
                let block = Block::from_id(block_ids[idx]);
                if block.is_solid() {
                    first_solid_from_top = Some(y);
                    break;
                }
            }
            let ceiling = first_solid_from_top.unwrap_or(0);
            for y in 0..y_extent {
                let idx = grid_index(x, y, z);
                let block = Block::from_id(block_ids[idx]);
                if block.is_solid() {
                    continue;
                }
                let level = if first_solid_from_top.is_none() || y > ceiling {
                    15
                } else {
                    let depth = (ceiling + 1).saturating_sub(y) as u8;
                    15_u8.saturating_sub(depth)
                };
                light_levels[idx] = level;
            }
        }
    }

    // Hook for emissive blocks: inject nearby light without a full flood-fill.
    for z in 0..xz_extent {
        for x in 0..xz_extent {
            for y in 0..y_extent {
                let idx = grid_index(x, y, z);
                let block = Block::from_id(block_ids[idx]);
                if !block.is_solid() {
                    continue;
                }
                let emission = block_light_emission(block);
                if emission == 0 {
                    continue;
                }
                if x > 0 {
                    let n = grid_index(x - 1, y, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if x + 1 < xz_extent {
                    let n = grid_index(x + 1, y, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if y > 0 {
                    let n = grid_index(x, y - 1, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if y + 1 < y_extent {
                    let n = grid_index(x, y + 1, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if z > 0 {
                    let n = grid_index(x, y, z - 1);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if z + 1 < xz_extent {
                    let n = grid_index(x, y, z + 1);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
            }
        }
    }

    light_levels
}

fn sample_light<F>(
    light_levels: &[u8],
    grid_index: &F,
    xz_extent: usize,
    y_extent: usize,
    x: usize,
    y: usize,
    z: usize,
) -> u8
where
    F: Fn(usize, usize, usize) -> usize,
{
    if x >= xz_extent || z >= xz_extent {
        return 15;
    }
    if y >= y_extent {
        return 15;
    }
    light_levels[grid_index(x, y, z)]
}

fn apply_light(color: [f32; 3], light_level: u8, face_shade: f32) -> [f32; 3] {
    let block_light = (light_level as f32 / 15.0).clamp(0.0, 1.0);
    let brightness = ((0.12 + block_light * 0.88) * face_shade).clamp(0.08, 1.1);
    [
        (color[0] * brightness).clamp(0.0, 1.0),
        (color[1] * brightness).clamp(0.0, 1.0),
        (color[2] * brightness).clamp(0.0, 1.0),
    ]
}

fn block_light_emission(block: Block) -> u8 {
    match block {
        Block::Air | Block::Grass | Block::Dirt | Block::Stone | Block::Deepslate | Block::DeepDark => 0,
    }
}

fn valid_sub_coords(divisions: u8, sx: u8, sy: u8, sz: u8) -> bool {
    (divisions == 3 || divisions == 6)
        && sx < divisions
        && sy < divisions
        && sz < divisions
}

fn add_sub_block_cube(vertices: &mut Vec<Vertex>, pos: SubBlockPos, block: Block, world: &World) {
    if !valid_sub_coords(pos.divisions, pos.sx, pos.sy, pos.sz) {
        return;
    }
    let step = 1.0 / pos.divisions as f32;
    let bx = pos.x as f32 + pos.sx as f32 * step;
    let by = pos.y as f32 + pos.sy as f32 * step;
    let bz = pos.z as f32 + pos.sz as f32 * step;
    let min = Vec3::new(bx, by, bz);
    let max = min + Vec3::splat(step);
    let (top_color, side_color, bottom_color) = world.palette_at(block, pos.x, pos.z);

    push_quad(
        vertices,
        [
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(max.x, max.y, min.z),
        ],
        top_color,
        Vec3::Y,
    );
    push_quad(
        vertices,
        [
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, min.y, max.z),
        ],
        bottom_color,
        -Vec3::Y,
    );
    push_quad(
        vertices,
        [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(min.x, max.y, min.z),
        ],
        side_color,
        -Vec3::X,
    );
    push_quad(
        vertices,
        [
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, max.y, max.z),
        ],
        side_color,
        Vec3::X,
    );
    push_quad(
        vertices,
        [
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
        ],
        side_color,
        -Vec3::Z,
    );
    push_quad(
        vertices,
        [
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ],
        side_color,
        Vec3::Z,
    );
}

fn wave_curve(seed: i64, along_axis: f32, x: f32, z: f32) -> f32 {
    let long_wave = (along_axis * 0.115).sin() * 0.95;
    let noise_wave = value_noise_2d(seed, x * 0.16, z * 0.16) * 1.25;
    long_wave + noise_wave
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

fn smooth_range(value: f32, edge0: f32, edge1: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smoothstep(t)
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

fn contains_world_y(y: i32) -> bool {
    (WORLD_MIN_Y..=WORLD_MAX_Y).contains(&y)
}

fn world_y_to_index(y: i32) -> Option<usize> {
    if contains_world_y(y) {
        Some((y - WORLD_MIN_Y) as usize)
    } else {
        None
    }
}

fn crust_block_for_y(y: i32) -> Block {
    if y < -128 {
        Block::DeepDark
    } else if y < 0 {
        Block::Deepslate
    } else {
        Block::Stone
    }
}

fn should_carve_cave(seed: i64, x: i64, y: i32, z: i64) -> bool {
    if !((WORLD_OVERWORLD_FLOOR - 96)..=(WORLD_OVERWORLD_FLOOR - 4)).contains(&y) {
        return false;
    }

    let xf = x as f32;
    let yf = y as f32;
    let zf = z as f32;

    let coarse_gate = value_noise_3d(
        seed.wrapping_add(0xD131_0BA6),
        xf * 0.018,
        yf * 0.022,
        zf * 0.018,
    );
    if coarse_gate < 0.18 {
        return false;
    }

    // Domain warp gives tunnels organic bends instead of grid-aligned blobs.
    let warp_x = value_noise_3d(
        seed.wrapping_add(0x9E37_79B9),
        xf * 0.012,
        yf * 0.016,
        zf * 0.012,
    ) * 9.0;
    let warp_z = value_noise_3d(
        seed.wrapping_add(0x243F_6A88),
        xf * 0.012 + 81.0,
        yf * 0.016 - 53.0,
        zf * 0.012 + 29.0,
    ) * 9.0;

    let wx = xf + warp_x;
    let wz = zf + warp_z;

    // Worm-like tunnels (ridged field around curving center-lines).
    let tunnel_path = value_noise_3d(
        seed.wrapping_add(0xB7E1_5163),
        wx * 0.030,
        yf * 0.028,
        wz * 0.030,
    );
    let tunnel_cross = value_noise_3d(
        seed.wrapping_add(0x94D0_49BB),
        wx * 0.060 + 117.0,
        yf * 0.040 - 41.0,
        wz * 0.060 - 73.0,
    );
    let worm_tunnel = (1.0 - (tunnel_path - tunnel_cross * 0.35).abs()).powf(2.2);

    // Chamber pockets to create larger caverns linked by tunnels.
    let chamber_field = value_noise_3d(
        seed.wrapping_add(0xA54F_F53A),
        wx * 0.016,
        yf * 0.020,
        wz * 0.016,
    );
    let chamber_detail = value_noise_3d(
        seed.wrapping_add(0x510E_527F),
        wx * 0.035 + 9.0,
        yf * 0.030 + 13.0,
        wz * 0.035 - 5.0,
    );
    let chambers = (chamber_field * 0.78 + chamber_detail * 0.22) > 0.58;

    let depth_boost = if y < 0 { 0.04 } else { 0.0 };
    worm_tunnel > 0.71 - depth_boost || chambers
}

fn value_noise_3d(seed: i64, x: f32, y: f32, z: f32) -> f32 {
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let z0 = z.floor() as i64;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let z1 = z0 + 1;

    let tx = smoothstep(x - x.floor());
    let ty = smoothstep(y - y.floor());
    let tz = smoothstep(z - z.floor());

    let c000 = hash3_to_unit(seed, x0, y0, z0);
    let c100 = hash3_to_unit(seed, x1, y0, z0);
    let c010 = hash3_to_unit(seed, x0, y1, z0);
    let c110 = hash3_to_unit(seed, x1, y1, z0);
    let c001 = hash3_to_unit(seed, x0, y0, z1);
    let c101 = hash3_to_unit(seed, x1, y0, z1);
    let c011 = hash3_to_unit(seed, x0, y1, z1);
    let c111 = hash3_to_unit(seed, x1, y1, z1);

    let x00 = lerp(c000, c100, tx);
    let x10 = lerp(c010, c110, tx);
    let x01 = lerp(c001, c101, tx);
    let x11 = lerp(c011, c111, tx);
    let y0 = lerp(x00, x10, ty);
    let y1 = lerp(x01, x11, ty);
    lerp(y0, y1, tz)
}

fn hash3_to_unit(seed: i64, x: i64, y: i64, z: i64) -> f32 {
    let mut h = seed as u64;
    h ^= (x as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    h ^= (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    let u = (h as f64) / (u64::MAX as f64);
    (u as f32) * 2.0 - 1.0
}

fn palette(block: Block, x: i64, z: i64, biome: BiomeKind) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let biome_tint = match biome {
        BiomeKind::Ocean => [0.62, 0.82, 1.14],
        BiomeKind::Meadow => [1.00, 1.00, 1.00],
        BiomeKind::Valley => [0.84, 1.08, 0.84],
        BiomeKind::Mountain => [0.78, 0.84, 0.92],
        BiomeKind::Desert => [1.22, 1.06, 0.70],
    };
    let tint = |color: [f32; 3]| -> [f32; 3] {
        [
            (color[0] * biome_tint[0]).clamp(0.0, 1.0),
            (color[1] * biome_tint[1]).clamp(0.0, 1.0),
            (color[2] * biome_tint[2]).clamp(0.0, 1.0),
        ]
    };
    match block {
        Block::Grass => {
            let top = if ((x + z) & 1) == 0 {
                [0.36, 0.78, 0.40]
            } else {
                [0.30, 0.70, 0.34]
            };
            (
                tint(top),
                tint([0.33, 0.25, 0.16]),
                tint([0.16, 0.22, 0.14]),
            )
        }
        Block::Dirt => (
            tint([0.40, 0.28, 0.18]),
            tint([0.35, 0.23, 0.14]),
            tint([0.22, 0.14, 0.09]),
        ),
        Block::Stone => (
            tint([0.55, 0.57, 0.60]),
            tint([0.48, 0.50, 0.53]),
            tint([0.35, 0.36, 0.39]),
        ),
        Block::Deepslate => (
            tint([0.34, 0.36, 0.39]),
            tint([0.28, 0.30, 0.33]),
            tint([0.22, 0.24, 0.27]),
        ),
        Block::DeepDark => (
            tint([0.06, 0.08, 0.09]),
            tint([0.04, 0.06, 0.07]),
            tint([0.02, 0.03, 0.04]),
        ),
        Block::Air => ([0.0; 3], [0.0; 3], [0.0; 3]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn height_stays_within_world_bounds() {
        let world = World::generate(64, 32, 64);
        for z in -128..=128 {
            for x in -128..=128 {
                let y = world.surface_height(x, z);
                assert!((WORLD_OVERWORLD_FLOOR..=(WORLD_MAX_Y - 4)).contains(&y));
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

    #[test]
    fn blended_biome_transitions_avoid_extreme_neighbor_jumps() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        let mut max_step = 0_i32;
        for z in -256..256 {
            for x in -256..256 {
                let h = world.surface_height(x, z);
                let hx = world.surface_height(x + 1, z);
                let hz = world.surface_height(x, z + 1);
                max_step = max_step.max((h - hx).abs());
                max_step = max_step.max((h - hz).abs());
            }
        }
        assert!(
            max_step <= 18,
            "found excessive adjacent step height {max_step}; blending likely regressed"
        );
    }

    #[test]
    fn chunk_edge_biomes_blend_with_mixture_when_neighbors_differ() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        let mut found_pair = None;
        'search: for cz in -24..=24 {
            for cx in -24..=24 {
                let left = world.biome_kind_for_chunk(cx, cz);
                let right = world.biome_kind_for_chunk(cx + 1, cz);
                if left != right {
                    found_pair = Some((cx, cz, left, right));
                    break 'search;
                }
            }
        }
        let Some((cx, cz, left, right)) = found_pair else {
            panic!("failed to find differing neighboring chunk biomes for blend test");
        };

        let edge_x = (cx + 1) * CHUNK_SIZE - 1;
        let mut left_count = 0_usize;
        let mut right_count = 0_usize;
        for local_z in 0..CHUNK_SIZE {
            for offset_x in -2..=2 {
                let kind = world.biome_at(edge_x + offset_x, cz * CHUNK_SIZE + local_z);
                if kind == left {
                    left_count += 1;
                }
                if kind == right {
                    right_count += 1;
                }
            }
        }
        assert!(
            left_count > 0 && right_count > 0,
            "edge mixture missing: left_count={left_count}, right_count={right_count}"
        );
    }

    #[test]
    fn lod_mesh_has_no_degenerate_or_invalid_triangles() {
        let world = World::generate(64, 32, 64);
        let origins = [(0, 0), (8, -8), (24, 24), (-40, 16)];

        for lod in 1..=3 {
            for origin in origins {
                let vertices = world.build_chunk_mesh_lod(origin, lod);
                assert_eq!(
                    vertices.len() % 3,
                    0,
                    "mesh triangle list must be 3-aligned for lod {lod} at {origin:?}"
                );

                for tri in vertices.chunks_exact(3) {
                    let a = v3(tri[0].position);
                    let b = v3(tri[1].position);
                    let c = v3(tri[2].position);
                    let area = (b - a).cross(c - a).length() * 0.5;
                    assert!(
                        area > 1e-4,
                        "degenerate triangle in lod {lod} at {origin:?}: {a:?} {b:?} {c:?}"
                    );
                    for v in [a, b, c] {
                        assert!(
                            v.is_finite(),
                            "non-finite vertex in lod {lod} at {origin:?}: {v:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn lod_top_surface_fully_covers_tile_area() {
        let world = World::generate(64, 32, 64);
        let origins = [(0, 0), (12, -12), (40, 8), (-32, -24)];

        for lod in 1..=3 {
            let span_blocks = CHUNK_SIZE * (1_i64 << lod);
            let expected_area = (span_blocks * span_blocks) as f32;
            for origin in origins {
                let vertices = world.build_chunk_mesh_lod(origin, lod);
                let mut top_area = 0.0_f32;
                for tri in vertices.chunks_exact(3) {
                    let normal = v3(tri[0].normal);
                    if normal.y < 0.99 {
                        continue;
                    }
                    let a = v3(tri[0].position);
                    let b = v3(tri[1].position);
                    let c = v3(tri[2].position);
                    let area_2d =
                        ((b.x - a.x) * (c.z - a.z) - (b.z - a.z) * (c.x - a.x)).abs() * 0.5;
                    top_area += area_2d;
                }
                let tolerance = expected_area * 0.0025;
                assert!(
                    (top_area - expected_area).abs() <= tolerance,
                    "lod {lod} at {origin:?} top coverage mismatch: got {top_area}, expected {expected_area}"
                );
            }
        }
    }

    #[test]
    fn skylight_decays_with_depth_when_overhang_blocks_direct_sun() {
        let xz_extent = 5_usize;
        let y_extent = 10_usize;
        let layer_stride = xz_extent * xz_extent;
        let grid_index = |x: usize, y: usize, z: usize| y * layer_stride + z * xz_extent + x;
        let mut block_ids = vec![Block::Air.to_id(); xz_extent * xz_extent * y_extent];

        // Build a 3x3 roof at y=7 over the center so direct skylight is blocked below it.
        for z in 1..=3 {
            for x in 1..=3 {
                block_ids[grid_index(x, 7, z)] = Block::Stone.to_id();
            }
        }

        let light = build_light_volume(&block_ids, xz_extent, y_extent);
        let center_near_roof = light[grid_index(2, 6, 2)];
        let center_deeper = light[grid_index(2, 2, 2)];
        let open_sky = light[grid_index(0, 6, 0)];

        assert!(open_sky >= 14, "expected near-full skylight under open sky");
        assert!(
            center_near_roof < open_sky,
            "expected occluded column to be darker than open sky; got center={center_near_roof}, open={open_sky}"
        );
        assert!(
            center_deeper < center_near_roof,
            "expected light to decay with depth under overhang; got near={center_near_roof}, deep={center_deeper}"
        );
    }

    #[test]
    fn chunk_mesh_shading_avoids_pure_black_vertices() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        let vertices = world.build_chunk_mesh((0, 0));
        assert!(!vertices.is_empty(), "expected a non-empty mesh");

        let mut min_luma = f32::MAX;
        for v in &vertices {
            let luma = v.color[0] * 0.299 + v.color[1] * 0.587 + v.color[2] * 0.114;
            min_luma = min_luma.min(luma);
        }
        assert!(
            min_luma > 0.01,
            "mesh contains near-black vertices, minimum luma was {min_luma}"
        );
    }

    #[test]
    fn depth_strata_follow_requested_y_bands() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        assert_eq!(world.block_at_i64(0, -200, 0), Block::DeepDark);
        assert_eq!(world.block_at_i64(0, -64, 0), Block::Deepslate);
        assert_eq!(world.block_at_i64(0, 64, 0), Block::Stone);
    }

    #[test]
    fn biome_layout_has_diversity_across_medium_region() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        let mut seen = [false; 5];
        for cz in -20..=20 {
            for cx in -20..=20 {
                let biome = world.biome_kind_for_chunk(cx, cz);
                seen[biome_index(biome)] = true;
            }
        }
        let unique = seen.into_iter().filter(|v| *v).count();
        assert!(
            unique >= 3,
            "expected at least 3 biome kinds over sample area, found {unique}"
        );
    }

    fn v3(value: [f32; 3]) -> Vec3 {
        Vec3::new(value[0], value[1], value[2])
    }
}
