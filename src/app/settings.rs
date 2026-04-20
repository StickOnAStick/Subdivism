use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::render::GraphicsSettings;

const SETTINGS_VERSION: u32 = 1;
const DEFAULT_APP_SETTINGS_PATH: &str = "settings/user.settings";

#[derive(Clone, Copy, Debug)]
pub(super) struct AppSettings {
    pub(super) render_distance_chunks: u32,
    pub(super) frame_cap_index: usize,
    pub(super) graphics_settings: GraphicsSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            render_distance_chunks: super::DEFAULT_RENDER_DISTANCE_CHUNKS,
            frame_cap_index: super::DEFAULT_FRAME_CAP_INDEX,
            graphics_settings: GraphicsSettings::default(),
        }
    }
}

impl AppSettings {
    pub(super) fn new(
        render_distance_chunks: u32,
        frame_cap_index: usize,
        graphics_settings: GraphicsSettings,
    ) -> Self {
        Self {
            render_distance_chunks,
            frame_cap_index,
            graphics_settings,
        }
        .clamped()
    }

    pub(super) fn load_from_file(path: impl AsRef<Path>) -> Result<Option<Self>, String> {
        let path = path.as_ref();
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(format!("failed to read {}: {err}", path.display())),
        };
        let settings = Self::from_kv_text(&raw)?;
        Ok(Some(settings))
    }

    pub(super) fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        fs::write(path, self.to_kv_text())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    pub(super) fn to_kv_text(self) -> String {
        let graphics = self.graphics_settings;
        [
            "# Subdivism app settings",
            &format!("version={SETTINGS_VERSION}"),
            &format!("render_distance_chunks={}", self.render_distance_chunks),
            &format!("frame_cap_index={}", self.frame_cap_index),
            &format!("shadows_enabled={}", graphics.shadows_enabled),
            &format!("fog_enabled={}", graphics.fog_enabled),
            &format!("atmosphere_enabled={}", graphics.atmosphere_enabled),
            &format!(
                "ldo_start_distance_chunks={}",
                graphics.ldo_start_distance_chunks
            ),
            &format!("shader_quality={:.6}", graphics.shader_quality),
            &format!("ambient_boost={:.6}", graphics.ambient_boost),
            &format!("shadow_softness={:.6}", graphics.shadow_softness),
            &format!("shadow_contrast={:.6}", graphics.shadow_contrast),
            &format!("far_shadow_lift={:.6}", graphics.far_shadow_lift),
            &format!("fog_strength={:.6}", graphics.fog_strength),
            &format!("fog_start={:.6}", graphics.fog_start),
            &format!("fog_end={:.6}", graphics.fog_end),
            &format!("atmosphere_strength={:.6}", graphics.atmosphere_strength),
            &format!("color_vibrance={:.6}", graphics.color_vibrance),
            &format!("ldo_detail_scale={:.6}", graphics.ldo_detail_scale),
        ]
        .join("\n")
            + "\n"
    }

    pub(super) fn from_kv_text(raw: &str) -> Result<Self, String> {
        let mut settings = Self::default();
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
                "version" => {
                    let parsed = parse_u32(value, key)?;
                    if parsed != SETTINGS_VERSION {
                        return Err(format!(
                            "unsupported settings version `{parsed}` (expected {SETTINGS_VERSION})"
                        ));
                    }
                }
                "render_distance_chunks" => {
                    settings.render_distance_chunks = parse_u32(value, key)?;
                }
                "frame_cap_index" => {
                    settings.frame_cap_index = parse_usize(value, key)?;
                }
                "shadows_enabled" => {
                    settings.graphics_settings.shadows_enabled = parse_bool(value, key)?;
                }
                "fog_enabled" => {
                    settings.graphics_settings.fog_enabled = parse_bool(value, key)?;
                }
                "atmosphere_enabled" => {
                    settings.graphics_settings.atmosphere_enabled = parse_bool(value, key)?;
                }
                "ldo_start_distance_chunks" => {
                    settings.graphics_settings.ldo_start_distance_chunks = parse_u32(value, key)?;
                }
                "shader_quality" => {
                    settings.graphics_settings.shader_quality = parse_f32(value, key)?;
                }
                "ambient_boost" => {
                    settings.graphics_settings.ambient_boost = parse_f32(value, key)?;
                }
                "shadow_softness" => {
                    settings.graphics_settings.shadow_softness = parse_f32(value, key)?;
                }
                "shadow_contrast" => {
                    settings.graphics_settings.shadow_contrast = parse_f32(value, key)?;
                }
                "far_shadow_lift" => {
                    settings.graphics_settings.far_shadow_lift = parse_f32(value, key)?;
                }
                "fog_strength" => {
                    settings.graphics_settings.fog_strength = parse_f32(value, key)?;
                }
                "fog_start" => {
                    settings.graphics_settings.fog_start = parse_f32(value, key)?;
                }
                "fog_end" => {
                    settings.graphics_settings.fog_end = parse_f32(value, key)?;
                }
                "atmosphere_strength" => {
                    settings.graphics_settings.atmosphere_strength = parse_f32(value, key)?;
                }
                "color_vibrance" => {
                    settings.graphics_settings.color_vibrance = parse_f32(value, key)?;
                }
                "ldo_detail_scale" => {
                    settings.graphics_settings.ldo_detail_scale = parse_f32(value, key)?;
                }
                _ => {}
            }
        }
        Ok(settings.clamped())
    }

    fn clamped(mut self) -> Self {
        let defaults = GraphicsSettings::default();

        self.render_distance_chunks = self.render_distance_chunks.clamp(
            super::MIN_RENDER_DISTANCE_CHUNKS,
            super::MAX_RENDER_DISTANCE_CHUNKS,
        );
        self.frame_cap_index = self
            .frame_cap_index
            .min(super::FRAME_CAP_PRESETS.len().saturating_sub(1));

        self.graphics_settings.ldo_start_distance_chunks =
            self.graphics_settings.ldo_start_distance_chunks.clamp(
                super::MIN_LDO_START_DISTANCE_CHUNKS,
                super::MAX_LDO_START_DISTANCE_CHUNKS,
            );
        self.graphics_settings.ldo_detail_scale = clamp_f32_or_default(
            self.graphics_settings.ldo_detail_scale,
            0.55,
            1.8,
            defaults.ldo_detail_scale,
        );
        self.graphics_settings.shader_quality = clamp_f32_or_default(
            self.graphics_settings.shader_quality,
            0.0,
            1.0,
            defaults.shader_quality,
        );
        self.graphics_settings.ambient_boost = clamp_f32_or_default(
            self.graphics_settings.ambient_boost,
            0.0,
            0.70,
            defaults.ambient_boost,
        );
        self.graphics_settings.shadow_softness = clamp_f32_or_default(
            self.graphics_settings.shadow_softness,
            0.1,
            1.5,
            defaults.shadow_softness,
        );
        self.graphics_settings.shadow_contrast = clamp_f32_or_default(
            self.graphics_settings.shadow_contrast,
            0.2,
            2.2,
            defaults.shadow_contrast,
        );
        self.graphics_settings.far_shadow_lift = clamp_f32_or_default(
            self.graphics_settings.far_shadow_lift,
            0.0,
            0.6,
            defaults.far_shadow_lift,
        );
        self.graphics_settings.fog_strength = clamp_f32_or_default(
            self.graphics_settings.fog_strength,
            0.0,
            1.2,
            defaults.fog_strength,
        );
        self.graphics_settings.fog_start = clamp_f32_or_default(
            self.graphics_settings.fog_start,
            16.0,
            1200.0,
            defaults.fog_start,
        );
        self.graphics_settings.fog_end = clamp_f32_or_default(
            self.graphics_settings.fog_end,
            24.0,
            2000.0,
            defaults.fog_end,
        );
        if self.graphics_settings.fog_end <= self.graphics_settings.fog_start + 1.0 {
            self.graphics_settings.fog_end = self.graphics_settings.fog_start + 1.0;
        }
        self.graphics_settings.atmosphere_strength = clamp_f32_or_default(
            self.graphics_settings.atmosphere_strength,
            0.0,
            1.0,
            defaults.atmosphere_strength,
        );
        self.graphics_settings.color_vibrance = clamp_f32_or_default(
            self.graphics_settings.color_vibrance,
            0.5,
            1.8,
            defaults.color_vibrance,
        );

        self
    }
}

pub(super) fn default_settings_path() -> PathBuf {
    PathBuf::from(DEFAULT_APP_SETTINGS_PATH)
}

fn parse_u32(input: &str, key: &str) -> Result<u32, String> {
    input
        .parse::<u32>()
        .map_err(|err| format!("invalid {key} `{input}`: {err}"))
}

fn parse_usize(input: &str, key: &str) -> Result<usize, String> {
    input
        .parse::<usize>()
        .map_err(|err| format!("invalid {key} `{input}`: {err}"))
}

fn parse_f32(input: &str, key: &str) -> Result<f32, String> {
    input
        .parse::<f32>()
        .map_err(|err| format!("invalid {key} `{input}`: {err}"))
}

fn parse_bool(input: &str, key: &str) -> Result<bool, String> {
    if input.eq_ignore_ascii_case("true") || input == "1" {
        return Ok(true);
    }
    if input.eq_ignore_ascii_case("false") || input == "0" {
        return Ok(false);
    }
    Err(format!("invalid {key} `{input}`: expected true/false or 1/0"))
}

fn clamp_f32_or_default(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_stable() {
        let mut settings = AppSettings::default();
        settings.render_distance_chunks = 30;
        settings.frame_cap_index = 1;
        settings.graphics_settings.fog_enabled = false;
        settings.graphics_settings.shader_quality = 0.4;

        let text = settings.to_kv_text();
        let parsed = AppSettings::from_kv_text(&text).expect("settings parse");

        assert_eq!(parsed.render_distance_chunks, 30);
        assert_eq!(parsed.frame_cap_index, 1);
        assert!(!parsed.graphics_settings.fog_enabled);
        assert!((parsed.graphics_settings.shader_quality - 0.4).abs() < 0.0001);
    }

    #[test]
    fn settings_parse_clamps_extreme_values() {
        let text = [
            "version=1",
            "render_distance_chunks=999",
            "frame_cap_index=999",
            "ldo_start_distance_chunks=999",
            "shader_quality=NaN",
            "fog_start=-99",
            "fog_end=2",
        ]
        .join("\n")
            + "\n";

        let parsed = AppSettings::from_kv_text(&text).expect("settings parse");
        let defaults = AppSettings::default();

        assert_eq!(
            parsed.render_distance_chunks,
            super::super::MAX_RENDER_DISTANCE_CHUNKS
        );
        assert_eq!(parsed.frame_cap_index, super::super::FRAME_CAP_PRESETS.len() - 1);
        assert_eq!(
            parsed.graphics_settings.ldo_start_distance_chunks,
            super::super::MAX_LDO_START_DISTANCE_CHUNKS
        );
        assert_eq!(
            parsed.graphics_settings.shader_quality,
            defaults.graphics_settings.shader_quality
        );
        assert!(
            parsed.graphics_settings.fog_end > parsed.graphics_settings.fog_start,
            "fog_end should stay ahead of fog_start"
        );
    }
}
