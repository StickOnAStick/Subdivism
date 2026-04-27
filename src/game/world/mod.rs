use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use glam::Vec3;

use crate::mesh::{Vertex, push_quad};

mod lighting;
mod meshing;
mod noise;
mod palette;
mod storage;
mod terrain_gen;

use noise::{smooth_range, value_noise_2d, value_noise_3d};
pub use storage::WorldCollisionView;
use storage::WorldStorage;

pub const CHUNK_SIZE: i64 = 16;
pub const WORLD_MIN_Y: i32 = -64;
pub const WORLD_MAX_Y: i32 = 320;
pub const WORLD_HEIGHT: i32 = WORLD_MAX_Y - WORLD_MIN_Y + 1;
pub const WORLD_OVERWORLD_FLOOR: i32 = 0;
pub const WORLD_SEA_LEVEL: i32 = 0;
pub const WORLD_SURFACE_MIN_Y: i32 = WORLD_MIN_Y + 2;
pub const DEFAULT_WORLD_SEED: i64 = 0x5EED_BA5E_u64 as i64;
pub const MAX_NATURAL_CAVE_DEPTH_BELOW_SURFACE: i32 = 200;
pub const WORLD_RENDER_FLOOR_Y: i32 = WORLD_MIN_Y;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Block {
    Air,
    Water,
    Grass,
    Dirt,
    Stone,
    Deepslate,
    DeepDark,
}

impl Block {
    pub fn is_solid(self) -> bool {
        !matches!(self, Self::Air | Self::Water)
    }

    fn to_id(self) -> u8 {
        self as u8
    }

    fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Water,
            2 => Self::Grass,
            3 => Self::Dirt,
            4 => Self::Stone,
            5 => Self::Deepslate,
            6 => Self::DeepDark,
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
    pub micro_scale: f32,
    pub micro_amplitude: f32,
    pub mountain_scale: f32,
    pub mountain_amplitude: f32,
    pub mountain_sharpness: f32,
    pub valley_scale: f32,
    pub valley_depth: f32,
    pub cliff_scale: f32,
    pub cliff_strength: f32,
    pub cliff_ledge_flatness: f32,
    pub cliff_recess_strength: f32,
    pub cliff_edge_rounding: f32,
    pub cliff_base_smoothing: f32,
    pub terrace_step: f32,
    pub biome_scale: f32,
    pub biome_region_scale: f32,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

const TERRAIN_PARAM_SPECS: [TerrainParamSpec; 31] = [
    TerrainParamSpec {
        key: "base_height",
        label: "BASE HEIGHT",
        min: -128.0,
        max: 256.0,
        step: 2.0,
    },
    TerrainParamSpec {
        key: "macro_scale",
        label: "MACRO SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "macro_amplitude",
        label: "MACRO AMPLITUDE",
        min: -128.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "detail_scale",
        label: "DETAIL SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "detail_amplitude",
        label: "DETAIL AMPLITUDE",
        min: -64.0,
        max: 64.0,
        step: 0.5,
    },
    TerrainParamSpec {
        key: "micro_scale",
        label: "MICRO SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "micro_amplitude",
        label: "MICRO AMPLITUDE",
        min: -24.0,
        max: 24.0,
        step: 0.25,
    },
    TerrainParamSpec {
        key: "mountain_scale",
        label: "MOUNTAIN SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "mountain_amplitude",
        label: "MOUNTAIN AMPLITUDE",
        min: 0.0,
        max: 192.0,
        step: 0.5,
    },
    TerrainParamSpec {
        key: "mountain_sharpness",
        label: "MOUNTAIN SHARPNESS",
        min: 0.0,
        max: 1.0,
        step: 0.02,
    },
    TerrainParamSpec {
        key: "valley_scale",
        label: "VALLEY SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "valley_depth",
        label: "VALLEY DEPTH",
        min: -128.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "cliff_scale",
        label: "CLIFF SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "cliff_strength",
        label: "CLIFF STRENGTH",
        min: 0.0,
        max: 1.0,
        step: 0.05,
    },
    TerrainParamSpec {
        key: "cliff_ledge_flatness",
        label: "CLIFF LEDGE FLATNESS",
        min: 0.0,
        max: 1.0,
        step: 0.05,
    },
    TerrainParamSpec {
        key: "cliff_recess_strength",
        label: "CLIFF RECESS STRENGTH",
        min: 0.0,
        max: 1.0,
        step: 0.05,
    },
    TerrainParamSpec {
        key: "cliff_edge_rounding",
        label: "CLIFF EDGE ROUNDING",
        min: 0.0,
        max: 1.0,
        step: 0.05,
    },
    TerrainParamSpec {
        key: "cliff_base_smoothing",
        label: "CLIFF BASE SMOOTHING",
        min: 0.0,
        max: 1.0,
        step: 0.05,
    },
    TerrainParamSpec {
        key: "terrace_step",
        label: "TERRACE STEP",
        min: -32.0,
        max: 64.0,
        step: 0.5,
    },
    TerrainParamSpec {
        key: "biome_scale",
        label: "BIOME SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "biome_region_scale",
        label: "BIOME REGION SIZE",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "biome_blend",
        label: "BIOME BLEND",
        min: 0.0,
        max: 1.0,
        step: 0.05,
    },
    TerrainParamSpec {
        key: "mountain_base_lift",
        label: "MOUNTAIN BASE LIFT",
        min: -128.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "desert_base_drop",
        label: "DESERT BASE DROP",
        min: -128.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "desert_dune_scale",
        label: "DUNE SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "desert_dune_amplitude",
        label: "DESERT DUNE AMPLITUDE",
        min: -64.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "ravine_scale",
        label: "RAVINE SPAN",
        min: 0.0,
        max: 1.0,
        step: 0.01,
    },
    TerrainParamSpec {
        key: "ravine_strength",
        label: "RAVINE STRENGTH",
        min: -64.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "ravine_width",
        label: "RAVINE WIDTH",
        min: -64.0,
        max: 128.0,
        step: 1.0,
    },
    TerrainParamSpec {
        key: "ravine_offset_x",
        label: "RAVINE OFFSET X",
        min: -131072.0,
        max: 131072.0,
        step: 16.0,
    },
    TerrainParamSpec {
        key: "ravine_offset_z",
        label: "RAVINE OFFSET Z",
        min: -131072.0,
        max: 131072.0,
        step: 16.0,
    },
];

impl TerrainConfig {
    pub fn parameter_specs() -> &'static [TerrainParamSpec] {
        &TERRAIN_PARAM_SPECS
    }

    pub fn parameter_keys() -> Vec<&'static str> {
        Self::parameter_specs()
            .iter()
            .map(|spec| spec.key)
            .collect()
    }

    pub fn parameter_spec(key: &str) -> Option<&'static TerrainParamSpec> {
        let key = key.to_ascii_lowercase();
        Self::parameter_specs().iter().find(|spec| spec.key == key)
    }

    pub fn named_param_value(&self, key: &str) -> Option<f32> {
        match key {
            "base_height" => Some(self.base_height),
            "macro_scale" => Some(self.macro_scale),
            "macro_amplitude" => Some(self.macro_amplitude),
            "detail_scale" => Some(self.detail_scale),
            "detail_amplitude" => Some(self.detail_amplitude),
            "micro_scale" => Some(self.micro_scale),
            "micro_amplitude" => Some(self.micro_amplitude),
            "mountain_scale" => Some(self.mountain_scale),
            "mountain_amplitude" => Some(self.mountain_amplitude),
            "mountain_sharpness" => Some(self.mountain_sharpness),
            "valley_scale" => Some(self.valley_scale),
            "valley_depth" => Some(self.valley_depth),
            "cliff_scale" => Some(self.cliff_scale),
            "cliff_strength" => Some(self.cliff_strength),
            "cliff_ledge_flatness" => Some(self.cliff_ledge_flatness),
            "cliff_recess_strength" => Some(self.cliff_recess_strength),
            "cliff_edge_rounding" => Some(self.cliff_edge_rounding),
            "cliff_base_smoothing" => Some(self.cliff_base_smoothing),
            "terrace_step" => Some(self.terrace_step),
            "biome_scale" => Some(self.biome_scale),
            "biome_region_scale" => Some(self.biome_region_scale),
            "biome_blend" => Some(self.biome_blend),
            "mountain_base_lift" => Some(self.mountain_base_lift),
            "desert_base_drop" => Some(self.desert_base_drop),
            "desert_dune_scale" => Some(self.desert_dune_scale),
            "desert_dune_amplitude" => Some(self.desert_dune_amplitude),
            "ravine_scale" => Some(self.ravine_scale),
            "ravine_strength" => Some(self.ravine_strength),
            "ravine_width" => Some(self.ravine_width),
            "ravine_offset_x" => Some(self.ravine_offset_x),
            "ravine_offset_z" => Some(self.ravine_offset_z),
            _ => None,
        }
    }

    pub fn zeroed() -> Self {
        Self {
            base_height: 0.0,
            macro_scale: 0.0,
            macro_amplitude: 0.0,
            detail_scale: 0.0,
            detail_amplitude: 0.0,
            micro_scale: 0.0,
            micro_amplitude: 0.0,
            mountain_scale: 0.0,
            mountain_amplitude: 0.0,
            mountain_sharpness: 0.0,
            valley_scale: 0.0,
            valley_depth: 0.0,
            cliff_scale: 0.0,
            cliff_strength: 0.0,
            cliff_ledge_flatness: 0.0,
            cliff_recess_strength: 0.0,
            cliff_edge_rounding: 0.0,
            cliff_base_smoothing: 0.0,
            terrace_step: 0.0,
            biome_scale: 0.0,
            biome_region_scale: 0.0,
            biome_blend: 0.0,
            mountain_base_lift: 0.0,
            desert_base_drop: 0.0,
            desert_dune_scale: 0.0,
            desert_dune_amplitude: 0.0,
            ravine_scale: 0.0,
            ravine_strength: 0.0,
            ravine_width: 0.0,
            ravine_offset_x: 0.0,
            ravine_offset_z: 0.0,
        }
    }

    pub fn balanced() -> Self {
        Self {
            base_height: 34.0,
            macro_scale: 0.70,
            macro_amplitude: 9.0,
            detail_scale: 0.58,
            detail_amplitude: 2.3,
            micro_scale: 0.36,
            micro_amplitude: 0.65,
            mountain_scale: 0.62,
            mountain_amplitude: 36.0,
            mountain_sharpness: 0.28,
            valley_scale: 0.72,
            valley_depth: 7.0,
            cliff_scale: 0.64,
            cliff_strength: 0.42,
            cliff_ledge_flatness: 0.55,
            cliff_recess_strength: 0.28,
            cliff_edge_rounding: 0.35,
            cliff_base_smoothing: 0.28,
            terrace_step: 1.8,
            biome_scale: 0.52,
            biome_region_scale: 0.55,
            biome_blend: 0.18,
            mountain_base_lift: 5.0,
            desert_base_drop: 6.0,
            desert_dune_scale: 0.48,
            desert_dune_amplitude: 2.5,
            ravine_scale: 0.67,
            ravine_strength: 2.6,
            ravine_width: 4.4,
            ravine_offset_x: 0.0,
            ravine_offset_z: 0.0,
        }
    }

    pub fn alpine() -> Self {
        Self {
            mountain_amplitude: 52.0,
            mountain_sharpness: 0.40,
            valley_depth: 4.0,
            cliff_strength: 0.62,
            cliff_ledge_flatness: 0.68,
            cliff_recess_strength: 0.34,
            cliff_edge_rounding: 0.24,
            cliff_base_smoothing: 0.18,
            mountain_base_lift: 10.0,
            desert_base_drop: 3.0,
            ravine_strength: 3.0,
            ..Self::balanced()
        }
    }

    pub fn canyon() -> Self {
        Self {
            base_height: 12.0,
            macro_amplitude: 6.0,
            micro_amplitude: 0.42,
            mountain_amplitude: 18.0,
            mountain_sharpness: 0.32,
            valley_depth: 12.0,
            cliff_strength: 0.78,
            cliff_ledge_flatness: 0.76,
            cliff_recess_strength: 0.52,
            cliff_edge_rounding: 0.14,
            cliff_base_smoothing: 0.12,
            terrace_step: 2.4,
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
            micro_amplitude: 0.78,
            mountain_amplitude: 24.0,
            mountain_sharpness: 0.20,
            valley_depth: 9.0,
            cliff_strength: 0.30,
            cliff_ledge_flatness: 0.42,
            cliff_recess_strength: 0.16,
            cliff_edge_rounding: 0.42,
            cliff_base_smoothing: 0.40,
            biome_blend: 0.24,
            ravine_strength: 1.5,
            ..Self::balanced()
        }
    }

    pub fn expansive() -> Self {
        Self {
            biome_region_scale: 1.0,
            biome_blend: 0.16,
            mountain_scale: 0.54,
            valley_scale: 0.64,
            ..Self::balanced()
        }
    }

    pub fn from_profile_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "balanced" => Some(Self::balanced()),
            "alpine" | "mountain" | "mountains" => Some(Self::alpine()),
            "canyon" | "cliff" | "cliffs" => Some(Self::canyon()),
            "valleylands" | "valley" | "valleys" => Some(Self::valleylands()),
            "expansive" | "continental" | "large" => Some(Self::expansive()),
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
            "micro_scale" => self.micro_scale = value,
            "micro_amplitude" => self.micro_amplitude = value,
            "mountain_scale" => self.mountain_scale = value,
            "mountain_amplitude" => self.mountain_amplitude = value,
            "mountain_sharpness" => self.mountain_sharpness = value,
            "valley_scale" => self.valley_scale = value,
            "valley_depth" => self.valley_depth = value,
            "cliff_scale" => self.cliff_scale = value,
            "cliff_strength" => self.cliff_strength = value,
            "cliff_ledge_flatness" => self.cliff_ledge_flatness = value,
            "cliff_recess_strength" => self.cliff_recess_strength = value,
            "cliff_edge_rounding" => self.cliff_edge_rounding = value,
            "cliff_base_smoothing" => self.cliff_base_smoothing = value,
            "terrace_step" => self.terrace_step = value,
            "biome_scale" => self.biome_scale = value,
            "biome_region_scale" => self.biome_region_scale = value,
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
        self.base_height = self.base_height.clamp(-128.0, 256.0);
        self.macro_scale = self.macro_scale.clamp(0.0, 1.0);
        self.macro_amplitude = self.macro_amplitude.clamp(-128.0, 128.0);
        self.detail_scale = self.detail_scale.clamp(0.0, 1.0);
        self.detail_amplitude = self.detail_amplitude.clamp(-64.0, 64.0);
        self.micro_scale = self.micro_scale.clamp(0.0, 1.0);
        self.micro_amplitude = self.micro_amplitude.clamp(-24.0, 24.0);
        self.mountain_scale = self.mountain_scale.clamp(0.0, 1.0);
        self.mountain_amplitude = self.mountain_amplitude.clamp(0.0, 192.0);
        self.mountain_sharpness = self.mountain_sharpness.clamp(0.0, 1.0);
        self.valley_scale = self.valley_scale.clamp(0.0, 1.0);
        self.valley_depth = self.valley_depth.clamp(-128.0, 128.0);
        self.cliff_scale = self.cliff_scale.clamp(0.0, 1.0);
        self.cliff_strength = self.cliff_strength.clamp(0.0, 1.0);
        self.cliff_ledge_flatness = self.cliff_ledge_flatness.clamp(0.0, 1.0);
        self.cliff_recess_strength = self.cliff_recess_strength.clamp(0.0, 1.0);
        self.cliff_edge_rounding = self.cliff_edge_rounding.clamp(0.0, 1.0);
        self.cliff_base_smoothing = self.cliff_base_smoothing.clamp(0.0, 1.0);
        self.terrace_step = self.terrace_step.clamp(-32.0, 64.0);
        self.biome_scale = self.biome_scale.clamp(0.0, 1.0);
        self.biome_region_scale = self.biome_region_scale.clamp(0.0, 1.0);
        self.biome_blend = self.biome_blend.clamp(0.0, 1.0);
        self.mountain_base_lift = self.mountain_base_lift.clamp(-128.0, 128.0);
        self.desert_base_drop = self.desert_base_drop.clamp(-128.0, 128.0);
        self.desert_dune_scale = self.desert_dune_scale.clamp(0.0, 1.0);
        self.desert_dune_amplitude = self.desert_dune_amplitude.clamp(-64.0, 128.0);
        self.ravine_scale = self.ravine_scale.clamp(0.0, 1.0);
        self.ravine_strength = self.ravine_strength.clamp(-64.0, 128.0);
        self.ravine_width = self.ravine_width.clamp(-64.0, 128.0);
        self.ravine_offset_x = self.ravine_offset_x.clamp(-131072.0, 131072.0);
        self.ravine_offset_z = self.ravine_offset_z.clamp(-131072.0, 131072.0);
    }
}

#[derive(Clone)]
pub struct World {
    storage: WorldStorage,
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
    Forest,
}

impl World {
    pub fn generate(size_x: i32, size_y: i32, size_z: i32) -> Self {
        let _requested_world_size = (size_x, size_y, size_z);
        Self::generate_default()
    }

    pub fn generate_default() -> Self {
        Self::generate_with_terrain(TerrainConfig::balanced())
    }

    pub fn generate_with_terrain(terrain: TerrainConfig) -> Self {
        Self::generate_with_terrain_and_seed(terrain, DEFAULT_WORLD_SEED)
    }

    pub fn generate_with_terrain_and_seed(terrain: TerrainConfig, seed: i64) -> Self {
        let mut world = Self {
            storage: WorldStorage {
                overrides: Arc::new(RwLock::new(HashMap::new())),
                sub_overrides: Arc::new(RwLock::new(storage::SubOverrideState::default())),
                spawn_point: Vec3::ZERO,
                center_column: (0, 0),
            },
            seed,
            terrain,
        };
        world.storage.spawn_point = world.spawn_point_for_column(0, 0).unwrap_or(Vec3::new(
            0.5,
            (WORLD_OVERWORLD_FLOOR + 3) as f32,
            0.5,
        ));
        world
    }

    pub fn spawn_point(&self) -> Vec3 {
        self.storage.spawn_point
    }

    pub fn is_out_of_bounds(&self, position: Vec3, margin: f32) -> bool {
        position.y < WORLD_MIN_Y as f32 - margin
    }

    pub fn world_to_chunk(x: i64, z: i64) -> (i64, i64) {
        (div_floor(x, CHUNK_SIZE), div_floor(z, CHUNK_SIZE))
    }

    pub fn center_column(&self) -> (i32, i32) {
        self.storage.center_column
    }

    pub fn terrain_seed(&self) -> i64 {
        self.seed
    }

    pub fn terrain_config(&self) -> TerrainConfig {
        self.terrain
    }
}

fn biome_index(kind: BiomeKind) -> usize {
    match kind {
        BiomeKind::Ocean => 0,
        BiomeKind::Meadow => 1,
        BiomeKind::Valley => 2,
        BiomeKind::Mountain => 3,
        BiomeKind::Desert => 4,
        BiomeKind::Forest => 5,
    }
}

fn biome_from_index(index: usize) -> BiomeKind {
    match index {
        0 => BiomeKind::Ocean,
        2 => BiomeKind::Valley,
        3 => BiomeKind::Mountain,
        4 => BiomeKind::Desert,
        5 => BiomeKind::Forest,
        _ => BiomeKind::Meadow,
    }
}

fn valid_sub_coords(divisions: u8, sx: u8, sy: u8, sz: u8) -> bool {
    (divisions == 3 || divisions == 6) && sx < divisions && sy < divisions && sz < divisions
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

fn crust_block_for_y(y: i32) -> Block {
    if y < -128 {
        Block::DeepDark
    } else if y < 0 {
        Block::Deepslate
    } else {
        Block::Stone
    }
}

fn should_carve_air(seed: i64, x: i64, y: i32, z: i64, surface_y: i32) -> bool {
    should_carve_cave(seed, x, y, z, surface_y)
        || should_force_surface_breakthrough(seed, x, y, z, surface_y)
}

fn should_force_surface_breakthrough(seed: i64, x: i64, y: i32, z: i64, surface_y: i32) -> bool {
    let depth_from_surface = (surface_y - y).max(0);
    if depth_from_surface > 5 {
        return false;
    }

    let xf = x as f32;
    let zf = z as f32;
    let lane = (1.0 - value_noise_2d(seed.wrapping_add(0xE19B_01AA), xf * 0.095, zf * 0.095).abs())
        .powf(2.2);
    if lane < 0.64 + depth_from_surface as f32 * 0.035 {
        return false;
    }

    let probe_depth = (depth_from_surface + 2).clamp(2, 5);
    let mut cave_below = should_carve_cave(seed, x, surface_y - probe_depth, z, surface_y);
    if !cave_below && depth_from_surface <= 2 && probe_depth < 5 {
        cave_below = should_carve_cave(seed, x, surface_y - (probe_depth + 1), z, surface_y);
    }
    if !cave_below {
        return false;
    }

    let yf = y as f32;
    let vertical_shape = (1.0
        - value_noise_3d(
            seed.wrapping_add(0xA6E8_7BAD),
            xf * 0.21,
            yf * 0.29,
            zf * 0.21,
        )
        .abs())
    .powf(2.4 + depth_from_surface as f32 * 0.2);
    let semi_random = lane * 0.58 + vertical_shape * 0.42;
    let threshold = 0.60 + depth_from_surface as f32 * 0.04;
    semi_random > threshold
}

fn should_carve_cave(seed: i64, x: i64, y: i32, z: i64, surface_y: i32) -> bool {
    let depth_from_surface = (surface_y - y).max(0);
    if depth_from_surface > MAX_NATURAL_CAVE_DEPTH_BELOW_SURFACE {
        return false;
    }

    let xf = x as f32;
    let yf = y as f32;
    let zf = z as f32;
    let depth_t = (depth_from_surface as f32 / 96.0).clamp(0.0, 1.0);

    // Surface entrances are usually narrow, with rare larger apertures.
    let entrance_lane = smooth_range(
        value_noise_2d(seed.wrapping_add(0x1C69_B3F7), xf * 0.0052, zf * 0.0052),
        0.74,
        0.93,
    );
    let throat_noise = value_noise_3d(
        seed.wrapping_add(0xC0AC_29B7),
        xf * 0.038,
        yf * 0.050,
        zf * 0.038,
    );
    let near_surface = smooth_range(22.0 - depth_from_surface as f32, 0.0, 22.0);
    let throat_shape = (1.0 - throat_noise.abs()).powf(4.8 + near_surface * 2.4);
    let narrow_surface_entrance = depth_from_surface <= 16 && entrance_lane * throat_shape > 0.42;

    let rare_surface_gate = smooth_range(
        value_noise_2d(seed.wrapping_add(0xA409_3822), xf * 0.0035, zf * 0.0035),
        0.88,
        0.985,
    );
    let occasional_large_surface_opening = depth_from_surface <= 14
        && rare_surface_gate > 0.62
        && (1.0
            - value_noise_3d(
                seed.wrapping_add(0x510E_527F),
                xf * 0.024,
                yf * 0.024,
                zf * 0.024,
            )
            .abs())
            > 0.78;

    // Surface connector bands create mostly small tunnels near the top.
    let connector_noise = value_noise_2d(seed.wrapping_add(0xDEAD_BEEF), xf * 0.010, zf * 0.010);
    let ravine_bias = (1.0
        - value_noise_2d(seed.wrapping_add(0x5BE0_CD19), xf * 0.0065, zf * 0.0065).abs())
    .powf(4.8);
    let connector_mask =
        (smooth_range(connector_noise, 0.58, 0.92) * 0.52 + ravine_bias * 0.28).clamp(0.0, 1.0);
    let surface_tunnel_shape = (1.0
        - value_noise_3d(
            seed.wrapping_add(0x9E37_79B9),
            xf * 0.034,
            yf * 0.030,
            zf * 0.034,
        )
        .abs())
    .powf(4.4 + near_surface * 2.2);
    let near_surface_small_tunnel =
        depth_from_surface <= 20 && connector_mask * surface_tunnel_shape > 0.56;

    if depth_from_surface <= 1 {
        return narrow_surface_entrance || occasional_large_surface_opening;
    }
    if depth_from_surface <= 12
        && (near_surface_small_tunnel
            || narrow_surface_entrance
            || occasional_large_surface_opening)
    {
        return true;
    }

    let coarse_gate = value_noise_3d(
        seed.wrapping_add(0xD131_0BA6),
        xf * 0.018,
        yf * 0.022,
        zf * 0.018,
    );
    let near_surface_tightening = smooth_range(32.0 - depth_from_surface as f32, 0.0, 32.0) * 0.44;
    let coarse_threshold =
        (0.16 + near_surface_tightening - connector_mask * 0.08 - (1.0 - depth_t) * 0.03).max(0.03);
    if coarse_gate < coarse_threshold {
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
    let chamber_score = chamber_field * 0.78 + chamber_detail * 0.22;
    let deep_chambers = chamber_score > 0.60 && depth_from_surface > 26;
    let rare_near_surface_chamber =
        depth_from_surface <= 16 && occasional_large_surface_opening && chamber_score > 0.78;

    let depth_boost = if y < 0 { 0.04 } else { 0.0 } + (1.0 - depth_t) * 0.02;
    if near_surface_small_tunnel && depth_from_surface <= 18 {
        return true;
    }

    let near_surface_penalty = smooth_range(34.0 - depth_from_surface as f32, 0.0, 34.0) * 0.34;
    let tunnel_threshold = 0.72 + near_surface_penalty - depth_boost - connector_mask * 0.06;
    let worm_open = worm_tunnel > tunnel_threshold;

    worm_open || deep_chambers || rare_near_surface_chamber
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
                assert!((WORLD_SURFACE_MIN_Y..=(WORLD_MAX_Y - 4)).contains(&y));
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

        let light = lighting::build_light_volume(
            &block_ids,
            xz_extent,
            y_extent,
            0,
            y_extent.saturating_sub(1),
        );
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
        let sample_has_block = |target_y: i32, expected: Block| -> bool {
            for z in (-64..=64).step_by(8) {
                for x in (-64..=64).step_by(8) {
                    if world.block_at_i64(x, target_y, z) == expected {
                        return true;
                    }
                }
            }
            false
        };

        if WORLD_MIN_Y <= -129 {
            assert!(
                sample_has_block(-129, Block::DeepDark),
                "expected at least one DeepDark sample at y=-129"
            );
        }
        assert!(
            sample_has_block(WORLD_MIN_Y.max(-64), Block::Deepslate),
            "expected at least one Deepslate sample near lower strata"
        );
        assert!(
            sample_has_block(WORLD_SEA_LEVEL + 24, Block::Stone),
            "expected at least one Stone sample at y={}",
            WORLD_SEA_LEVEL + 24
        );
    }

    #[test]
    fn biome_layout_has_diversity_across_medium_region() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        let mut seen = [false; 6];
        for cz in -64..=64 {
            for cx in -64..=64 {
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

    #[test]
    fn shallow_columns_include_natural_cave_or_tunnel_entries() {
        let world = World::generate_with_terrain(TerrainConfig::balanced());
        let mut columns_with_openings = 0_u32;
        let mut sampled_columns = 0_u32;
        for z in (-192..=192).step_by(8) {
            for x in (-192..=192).step_by(8) {
                sampled_columns += 1;
                let surface = world.surface_height(x, z);
                let top = (surface - 6).max(WORLD_MIN_Y);
                let bottom = (surface - 52).max(WORLD_MIN_Y);
                let mut found_opening = false;
                for y in (bottom..=top).rev() {
                    if world.block_at_i64(x, y, z) == Block::Air {
                        found_opening = true;
                        break;
                    }
                }
                if found_opening {
                    columns_with_openings += 1;
                }
            }
        }
        assert!(
            columns_with_openings >= sampled_columns / 20,
            "expected at least 5 percent of sampled columns to expose shallow cave/tunnel access, found {columns_with_openings} / {sampled_columns}"
        );
    }

    fn v3(value: [f32; 3]) -> Vec3 {
        Vec3::new(value[0], value[1], value[2])
    }
}
