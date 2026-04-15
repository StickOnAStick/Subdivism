use super::world::{TerrainConfig, WORLD_MAX_Y, WORLD_OVERWORLD_FLOOR};

pub struct TerrainParamRegistry;

impl TerrainParamRegistry {
    pub fn value(cfg: &TerrainConfig, key: &str) -> f32 {
        match key {
            "base_height" => cfg.base_height,
            "macro_scale" => cfg.macro_scale,
            "macro_amplitude" => cfg.macro_amplitude,
            "detail_scale" => cfg.detail_scale,
            "detail_amplitude" => cfg.detail_amplitude,
            "mountain_scale" => cfg.mountain_scale,
            "mountain_amplitude" => cfg.mountain_amplitude,
            "valley_scale" => cfg.valley_scale,
            "valley_depth" => cfg.valley_depth,
            "cliff_scale" => cfg.cliff_scale,
            "cliff_strength" => cfg.cliff_strength,
            "terrace_step" => cfg.terrace_step,
            "biome_scale" => cfg.biome_scale,
            "biome_blend" => cfg.biome_blend,
            "mountain_base_lift" => cfg.mountain_base_lift,
            "desert_base_drop" => cfg.desert_base_drop,
            "desert_dune_scale" => cfg.desert_dune_scale,
            "desert_dune_amplitude" => cfg.desert_dune_amplitude,
            "ravine_scale" => cfg.ravine_scale,
            "ravine_strength" => cfg.ravine_strength,
            "ravine_width" => cfg.ravine_width,
            "ravine_offset_x" => cfg.ravine_offset_x,
            "ravine_offset_z" => cfg.ravine_offset_z,
            _ => 0.0,
        }
    }

    pub fn step(key: &str) -> f32 {
        match key {
            "base_height" => 0.5,
            "macro_scale" => 0.0005,
            "macro_amplitude" => 0.4,
            "detail_scale" => 0.001,
            "detail_amplitude" => 0.2,
            "mountain_scale" => 0.0005,
            "mountain_amplitude" => 0.5,
            "valley_scale" => 0.0005,
            "valley_depth" => 0.4,
            "cliff_scale" => 0.0005,
            "cliff_strength" => 0.02,
            "terrace_step" => 0.1,
            "biome_scale" => 0.0002,
            "biome_blend" => 0.01,
            "mountain_base_lift" => 0.3,
            "desert_base_drop" => 0.3,
            "desert_dune_scale" => 0.001,
            "desert_dune_amplitude" => 0.2,
            "ravine_scale" => 0.0005,
            "ravine_strength" => 0.2,
            "ravine_width" => 0.1,
            "ravine_offset_x" => 1.0,
            "ravine_offset_z" => 1.0,
            _ => 0.1,
        }
    }

    pub fn min(key: &str) -> f32 {
        match key {
            "base_height" => (WORLD_OVERWORLD_FLOOR + 8) as f32,
            "macro_scale" => 0.0015,
            "macro_amplitude" => 0.0,
            "detail_scale" => 0.005,
            "detail_amplitude" => 0.0,
            "mountain_scale" => 0.0015,
            "mountain_amplitude" => 0.0,
            "valley_scale" => 0.0015,
            "valley_depth" => 0.0,
            "cliff_scale" => 0.004,
            "cliff_strength" => 0.0,
            "terrace_step" => 0.4,
            "biome_scale" => 0.0008,
            "biome_blend" => 0.02,
            "mountain_base_lift" => -8.0,
            "desert_base_drop" => 0.0,
            "desert_dune_scale" => 0.005,
            "desert_dune_amplitude" => 0.0,
            "ravine_scale" => 0.001,
            "ravine_strength" => 0.0,
            "ravine_width" => 1.0,
            "ravine_offset_x" => -4096.0,
            "ravine_offset_z" => -4096.0,
            _ => -1.0,
        }
    }

    pub fn max(key: &str) -> f32 {
        match key {
            "base_height" => (WORLD_MAX_Y - 8) as f32,
            "macro_scale" => 0.08,
            "macro_amplitude" => 30.0,
            "detail_scale" => 0.30,
            "detail_amplitude" => 12.0,
            "mountain_scale" => 0.08,
            "mountain_amplitude" => 40.0,
            "valley_scale" => 0.08,
            "valley_depth" => 24.0,
            "cliff_scale" => 0.16,
            "cliff_strength" => 1.0,
            "terrace_step" => 10.0,
            "biome_scale" => 0.03,
            "biome_blend" => 0.45,
            "mountain_base_lift" => 20.0,
            "desert_base_drop" => 20.0,
            "desert_dune_scale" => 0.20,
            "desert_dune_amplitude" => 12.0,
            "ravine_scale" => 0.08,
            "ravine_strength" => 16.0,
            "ravine_width" => 16.0,
            "ravine_offset_x" => 4096.0,
            "ravine_offset_z" => 4096.0,
            _ => 1.0,
        }
    }

    pub fn label(key: &str) -> String {
        key.chars()
            .map(|ch| {
                if ch == '_' {
                    ' '
                } else {
                    ch.to_ascii_uppercase()
                }
            })
            .collect::<String>()
    }
}
