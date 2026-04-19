use std::{
    fs,
    path::{Path, PathBuf},
};

use super::world::{DEFAULT_WORLD_SEED, TerrainConfig};

#[derive(Clone, Debug)]
pub struct TerrainRecipe {
    pub name: String,
    pub description: String,
    pub profile: String,
    pub seed: i64,
    pub terrain: TerrainConfig,
    pub source_path: Option<PathBuf>,
}

impl TerrainRecipe {
    pub fn balanced(seed: i64) -> Self {
        Self {
            name: "balanced_world".to_string(),
            description: String::new(),
            profile: "balanced".to_string(),
            seed,
            terrain: TerrainConfig::balanced(),
            source_path: None,
        }
    }

    pub fn default_seeded() -> Self {
        Self::balanced(DEFAULT_WORLD_SEED)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let mut recipe = Self::from_kv_text(&raw)?;
        recipe.source_path = Some(path.to_path_buf());
        Ok(recipe)
    }

    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        fs::write(path, self.to_kv_text())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    pub fn to_kv_text(&self) -> String {
        let terrain = self.terrain;
        [
            "# Subdivism terrain recipe",
            "version=1",
            &format!("name={}", self.name),
            &format!("description={}", self.description),
            &format!("profile={}", self.profile),
            &format!("seed={}", self.seed),
            &format!("base_height={:.6}", terrain.base_height),
            &format!("macro_scale={:.6}", terrain.macro_scale),
            &format!("macro_amplitude={:.6}", terrain.macro_amplitude),
            &format!("detail_scale={:.6}", terrain.detail_scale),
            &format!("detail_amplitude={:.6}", terrain.detail_amplitude),
            &format!("micro_scale={:.6}", terrain.micro_scale),
            &format!("micro_amplitude={:.6}", terrain.micro_amplitude),
            &format!("mountain_scale={:.6}", terrain.mountain_scale),
            &format!("mountain_amplitude={:.6}", terrain.mountain_amplitude),
            &format!("mountain_sharpness={:.6}", terrain.mountain_sharpness),
            &format!("valley_scale={:.6}", terrain.valley_scale),
            &format!("valley_depth={:.6}", terrain.valley_depth),
            &format!("cliff_scale={:.6}", terrain.cliff_scale),
            &format!("cliff_strength={:.6}", terrain.cliff_strength),
            &format!("cliff_ledge_flatness={:.6}", terrain.cliff_ledge_flatness),
            &format!("cliff_recess_strength={:.6}", terrain.cliff_recess_strength),
            &format!("cliff_edge_rounding={:.6}", terrain.cliff_edge_rounding),
            &format!("cliff_base_smoothing={:.6}", terrain.cliff_base_smoothing),
            &format!("terrace_step={:.6}", terrain.terrace_step),
            &format!("biome_scale={:.6}", terrain.biome_scale),
            &format!("biome_blend={:.6}", terrain.biome_blend),
            &format!("mountain_base_lift={:.6}", terrain.mountain_base_lift),
            &format!("desert_base_drop={:.6}", terrain.desert_base_drop),
            &format!("desert_dune_scale={:.6}", terrain.desert_dune_scale),
            &format!("desert_dune_amplitude={:.6}", terrain.desert_dune_amplitude),
            &format!("ravine_scale={:.6}", terrain.ravine_scale),
            &format!("ravine_strength={:.6}", terrain.ravine_strength),
            &format!("ravine_width={:.6}", terrain.ravine_width),
            &format!("ravine_offset_x={:.6}", terrain.ravine_offset_x),
            &format!("ravine_offset_z={:.6}", terrain.ravine_offset_z),
        ]
        .join("\n")
            + "\n"
    }

    pub fn from_kv_text(raw: &str) -> Result<Self, String> {
        let mut recipe = Self::default_seeded();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "name" => recipe.name = value.to_string(),
                "description" => recipe.description = value.to_string(),
                "profile" => {
                    recipe.profile = value.to_string();
                    if let Some(profile) = TerrainConfig::from_profile_name(value) {
                        recipe.terrain = profile;
                    }
                }
                "seed" => {
                    recipe.seed = value
                        .parse::<i64>()
                        .map_err(|err| format!("invalid seed `{value}`: {err}"))?;
                }
                "base_height" => parse_f32(value, &mut recipe.terrain.base_height, key)?,
                "macro_scale" => parse_f32(value, &mut recipe.terrain.macro_scale, key)?,
                "macro_amplitude" => parse_f32(value, &mut recipe.terrain.macro_amplitude, key)?,
                "detail_scale" => parse_f32(value, &mut recipe.terrain.detail_scale, key)?,
                "detail_amplitude" => parse_f32(value, &mut recipe.terrain.detail_amplitude, key)?,
                "micro_scale" => parse_f32(value, &mut recipe.terrain.micro_scale, key)?,
                "micro_amplitude" => parse_f32(value, &mut recipe.terrain.micro_amplitude, key)?,
                "mountain_scale" => parse_f32(value, &mut recipe.terrain.mountain_scale, key)?,
                "mountain_amplitude" => {
                    parse_f32(value, &mut recipe.terrain.mountain_amplitude, key)?
                }
                "mountain_sharpness" => {
                    parse_f32(value, &mut recipe.terrain.mountain_sharpness, key)?
                }
                "valley_scale" => parse_f32(value, &mut recipe.terrain.valley_scale, key)?,
                "valley_depth" => parse_f32(value, &mut recipe.terrain.valley_depth, key)?,
                "cliff_scale" => parse_f32(value, &mut recipe.terrain.cliff_scale, key)?,
                "cliff_strength" => parse_f32(value, &mut recipe.terrain.cliff_strength, key)?,
                "cliff_ledge_flatness" => {
                    parse_f32(value, &mut recipe.terrain.cliff_ledge_flatness, key)?
                }
                "cliff_recess_strength" => {
                    parse_f32(value, &mut recipe.terrain.cliff_recess_strength, key)?
                }
                "cliff_edge_rounding" => {
                    parse_f32(value, &mut recipe.terrain.cliff_edge_rounding, key)?
                }
                "cliff_base_smoothing" => {
                    parse_f32(value, &mut recipe.terrain.cliff_base_smoothing, key)?
                }
                "terrace_step" => parse_f32(value, &mut recipe.terrain.terrace_step, key)?,
                "biome_scale" => parse_f32(value, &mut recipe.terrain.biome_scale, key)?,
                "biome_blend" => parse_f32(value, &mut recipe.terrain.biome_blend, key)?,
                "mountain_base_lift" => {
                    parse_f32(value, &mut recipe.terrain.mountain_base_lift, key)?
                }
                "desert_base_drop" => parse_f32(value, &mut recipe.terrain.desert_base_drop, key)?,
                "desert_dune_scale" => {
                    parse_f32(value, &mut recipe.terrain.desert_dune_scale, key)?
                }
                "desert_dune_amplitude" => {
                    parse_f32(value, &mut recipe.terrain.desert_dune_amplitude, key)?
                }
                "ravine_scale" => parse_f32(value, &mut recipe.terrain.ravine_scale, key)?,
                "ravine_strength" => parse_f32(value, &mut recipe.terrain.ravine_strength, key)?,
                "ravine_width" => parse_f32(value, &mut recipe.terrain.ravine_width, key)?,
                "ravine_offset_x" => parse_f32(value, &mut recipe.terrain.ravine_offset_x, key)?,
                "ravine_offset_z" => parse_f32(value, &mut recipe.terrain.ravine_offset_z, key)?,
                _ => {}
            }
        }
        recipe.terrain.clamp_reasonable();
        Ok(recipe)
    }
}

fn parse_f32(input: &str, out: &mut f32, key: &str) -> Result<(), String> {
    *out = input
        .parse::<f32>()
        .map_err(|err| format!("invalid {key} `{input}`: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_round_trip_stable() {
        let mut recipe = TerrainRecipe::balanced(42);
        recipe.name = "test".to_string();
        recipe.description = "rolling hills".to_string();
        recipe.terrain.valley_depth = 7.25;
        let text = recipe.to_kv_text();
        let parsed = TerrainRecipe::from_kv_text(&text).expect("recipe parse");
        assert_eq!(parsed.seed, 42);
        assert_eq!(parsed.name, "test");
        assert!((parsed.terrain.valley_depth - 7.25).abs() < 0.001);
    }
}
