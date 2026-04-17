use super::world::TerrainConfig;

pub struct TerrainParamRegistry;

impl TerrainParamRegistry {
    pub fn value(cfg: &TerrainConfig, key: &str) -> f32 {
        cfg.named_param_value(key).unwrap_or(0.0)
    }

    pub fn step(key: &str) -> f32 {
        TerrainConfig::parameter_spec(key)
            .map(|spec| spec.step)
            .unwrap_or(0.1)
    }

    pub fn min(key: &str) -> f32 {
        TerrainConfig::parameter_spec(key)
            .map(|spec| spec.min)
            .unwrap_or(-1.0)
    }

    pub fn max(key: &str) -> f32 {
        TerrainConfig::parameter_spec(key)
            .map(|spec| spec.max)
            .unwrap_or(1.0)
    }

    pub fn label(key: &str) -> String {
        TerrainConfig::parameter_spec(key)
            .map(|spec| spec.label.to_string())
            .unwrap_or_else(|| {
                key.chars()
                    .map(|ch| {
                        if ch == '_' {
                            ' '
                        } else {
                            ch.to_ascii_uppercase()
                        }
                    })
                    .collect::<String>()
            })
    }
}
