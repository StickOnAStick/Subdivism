use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct TextureEntry {
    pub id: String,
    pub path: String,
    pub tint: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct BiomeEntry {
    pub id: String,
    pub terrain_profile: String,
    pub top_texture: String,
    pub side_texture: String,
    pub bottom_texture: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssetRegistry {
    pub textures: Vec<TextureEntry>,
    pub biomes: Vec<BiomeEntry>,
}

impl AssetRegistry {
    pub fn default_path() -> PathBuf {
        PathBuf::from("terrain/assets.registry")
    }

    pub fn with_defaults() -> Self {
        Self {
            textures: vec![
                TextureEntry {
                    id: "dirt".to_string(),
                    path: "textures/dirt.png".to_string(),
                    tint: [1.0, 1.0, 1.0],
                },
                TextureEntry {
                    id: "grass_top".to_string(),
                    path: "textures/grass_top.png".to_string(),
                    tint: [1.0, 1.0, 1.0],
                },
                TextureEntry {
                    id: "stone".to_string(),
                    path: "textures/stone.png".to_string(),
                    tint: [1.0, 1.0, 1.0],
                },
            ],
            biomes: vec![
                BiomeEntry {
                    id: "dry".to_string(),
                    terrain_profile: "canyon".to_string(),
                    top_texture: "dirt".to_string(),
                    side_texture: "dirt".to_string(),
                    bottom_texture: "stone".to_string(),
                },
                BiomeEntry {
                    id: "meadow".to_string(),
                    terrain_profile: "balanced".to_string(),
                    top_texture: "grass_top".to_string(),
                    side_texture: "dirt".to_string(),
                    bottom_texture: "dirt".to_string(),
                },
                BiomeEntry {
                    id: "rocky".to_string(),
                    terrain_profile: "alpine".to_string(),
                    top_texture: "stone".to_string(),
                    side_texture: "stone".to_string(),
                    bottom_texture: "stone".to_string(),
                },
            ],
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        Self::from_text(&raw)
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::with_defaults());
        }
        Self::from_file(path)
    }

    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        fs::write(path, self.to_text())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push("# Subdivism asset registry".to_string());
        lines.push("version=1".to_string());
        lines.push(String::new());
        for texture in &self.textures {
            lines.push(format!(
                "texture,{},{},{:.4},{:.4},{:.4}",
                texture.id, texture.path, texture.tint[0], texture.tint[1], texture.tint[2]
            ));
        }
        lines.push(String::new());
        for biome in &self.biomes {
            lines.push(format!(
                "biome,{},{},{},{},{}",
                biome.id,
                biome.terrain_profile,
                biome.top_texture,
                biome.side_texture,
                biome.bottom_texture
            ));
        }
        lines.join("\n") + "\n"
    }

    pub fn from_text(raw: &str) -> Result<Self, String> {
        let mut registry = Self::default();
        for (line_no, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("version=") {
                continue;
            }
            let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
            if fields.is_empty() {
                continue;
            }
            match fields[0] {
                "texture" => {
                    if fields.len() != 6 {
                        return Err(format!(
                            "line {}: texture entries require 6 fields",
                            line_no + 1
                        ));
                    }
                    registry.upsert_texture(TextureEntry {
                        id: fields[1].to_string(),
                        path: fields[2].to_string(),
                        tint: [
                            parse_f32(fields[3], line_no + 1, "tint_r")?,
                            parse_f32(fields[4], line_no + 1, "tint_g")?,
                            parse_f32(fields[5], line_no + 1, "tint_b")?,
                        ],
                    });
                }
                "biome" => {
                    if fields.len() != 6 {
                        return Err(format!(
                            "line {}: biome entries require 6 fields",
                            line_no + 1
                        ));
                    }
                    registry.upsert_biome(BiomeEntry {
                        id: fields[1].to_string(),
                        terrain_profile: fields[2].to_string(),
                        top_texture: fields[3].to_string(),
                        side_texture: fields[4].to_string(),
                        bottom_texture: fields[5].to_string(),
                    });
                }
                _ => {}
            }
        }
        Ok(registry)
    }

    pub fn upsert_texture(&mut self, texture: TextureEntry) {
        if let Some(existing) = self.textures.iter_mut().find(|t| t.id == texture.id) {
            *existing = texture;
        } else {
            self.textures.push(texture);
            self.textures.sort_by(|a, b| a.id.cmp(&b.id));
        }
    }

    pub fn remove_texture(&mut self, id: &str) {
        self.textures.retain(|t| t.id != id);
    }

    pub fn upsert_biome(&mut self, biome: BiomeEntry) {
        if let Some(existing) = self.biomes.iter_mut().find(|b| b.id == biome.id) {
            *existing = biome;
        } else {
            self.biomes.push(biome);
            self.biomes.sort_by(|a, b| a.id.cmp(&b.id));
        }
    }

    pub fn remove_biome(&mut self, id: &str) {
        self.biomes.retain(|b| b.id != id);
    }
}

fn parse_f32(input: &str, line_no: usize, field: &str) -> Result<f32, String> {
    input
        .parse::<f32>()
        .map_err(|err| format!("line {line_no}: invalid {field} `{input}`: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trip() {
        let registry = AssetRegistry::with_defaults();
        let text = registry.to_text();
        let parsed = AssetRegistry::from_text(&text).expect("registry parse");
        assert_eq!(registry, parsed);
    }
}
