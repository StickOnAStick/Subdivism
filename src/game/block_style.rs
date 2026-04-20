use std::{
    fs,
    path::{Path, PathBuf},
};

use super::world::Block;

pub type Color = [f32; 3];
pub const DEFAULT_BLOCK_STYLE_PATH: &str = "terrain/block_styles.lab";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFace {
    Top,
    Side,
    Bottom,
}

impl TextureFace {
    pub const ALL: [Self; 3] = [Self::Top, Self::Side, Self::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "TOP",
            Self::Side => "SIDE",
            Self::Bottom => "BOTTOM",
        }
    }

    pub fn cycle(self, delta: i32) -> Self {
        let idx = match self {
            Self::Top => 0_i32,
            Self::Side => 1,
            Self::Bottom => 2,
        };
        let next = (idx + delta).rem_euclid(Self::ALL.len() as i32);
        Self::ALL[next as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureBrush {
    Lighten,
    Darken,
    Saturate,
    Desaturate,
    Warm,
    Cool,
    Shade,
    Noise,
}

impl TextureBrush {
    pub const ALL: [Self; 8] = [
        Self::Lighten,
        Self::Darken,
        Self::Saturate,
        Self::Desaturate,
        Self::Warm,
        Self::Cool,
        Self::Shade,
        Self::Noise,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lighten => "LIGHTEN",
            Self::Darken => "DARKEN",
            Self::Saturate => "SATURATE",
            Self::Desaturate => "DESATURATE",
            Self::Warm => "WARM",
            Self::Cool => "COOL",
            Self::Shade => "SHADE",
            Self::Noise => "NOISE",
        }
    }

    pub fn cycle(self, delta: i32) -> Self {
        let idx = Self::ALL
            .iter()
            .position(|brush| *brush == self)
            .unwrap_or(0) as i32;
        let next = (idx + delta).rem_euclid(Self::ALL.len() as i32);
        Self::ALL[next as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockStyle {
    pub top: Color,
    pub side: Color,
    pub bottom: Color,
    pub biome_tint_strength: f32,
    pub grain_strength: f32,
}

impl BlockStyle {
    pub fn face_color(self, face: TextureFace) -> Color {
        match face {
            TextureFace::Top => self.top,
            TextureFace::Side => self.side,
            TextureFace::Bottom => self.bottom,
        }
    }

    pub fn set_face_color(&mut self, face: TextureFace, color: Color) {
        match face {
            TextureFace::Top => self.top = color,
            TextureFace::Side => self.side = color,
            TextureFace::Bottom => self.bottom = color,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockStyleBook {
    styles: [BlockStyle; Block::COUNT],
}

impl Default for BlockStyleBook {
    fn default() -> Self {
        Self {
            styles: [
                style_for_block(Block::Air),
                style_for_block(Block::Water),
                style_for_block(Block::Grass),
                style_for_block(Block::Dirt),
                style_for_block(Block::Stone),
                style_for_block(Block::Deepslate),
                style_for_block(Block::DeepDark),
                style_for_block(Block::CustomA),
                style_for_block(Block::CustomB),
                style_for_block(Block::CustomC),
            ],
        }
    }
}

impl BlockStyleBook {
    pub fn default_path() -> PathBuf {
        PathBuf::from(DEFAULT_BLOCK_STYLE_PATH)
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
            return Ok(Self::default());
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
        lines.push("# Subdivism block style book".to_string());
        lines.push("version=1".to_string());
        lines.push(String::new());
        for block in Block::all().iter().copied() {
            let style = self.style(block);
            lines.push(format!(
                "style,{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
                block_key(block),
                style.top[0],
                style.top[1],
                style.top[2],
                style.side[0],
                style.side[1],
                style.side[2],
                style.bottom[0],
                style.bottom[1],
                style.bottom[2],
                style.biome_tint_strength,
                style.grain_strength
            ));
        }
        lines.join("\n") + "\n"
    }

    pub fn from_text(raw: &str) -> Result<Self, String> {
        let mut book = Self::default();
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
                "style" => {
                    if fields.len() != 13 {
                        return Err(format!(
                            "line {}: style entries require 13 fields",
                            line_no + 1
                        ));
                    }
                    let Some(block) = block_from_key(fields[1]) else {
                        return Err(format!(
                            "line {}: unknown block key `{}`",
                            line_no + 1,
                            fields[1]
                        ));
                    };
                    let style = BlockStyle {
                        top: [
                            parse_f32(fields[2], line_no + 1, "top_r")?,
                            parse_f32(fields[3], line_no + 1, "top_g")?,
                            parse_f32(fields[4], line_no + 1, "top_b")?,
                        ],
                        side: [
                            parse_f32(fields[5], line_no + 1, "side_r")?,
                            parse_f32(fields[6], line_no + 1, "side_g")?,
                            parse_f32(fields[7], line_no + 1, "side_b")?,
                        ],
                        bottom: [
                            parse_f32(fields[8], line_no + 1, "bottom_r")?,
                            parse_f32(fields[9], line_no + 1, "bottom_g")?,
                            parse_f32(fields[10], line_no + 1, "bottom_b")?,
                        ],
                        biome_tint_strength: parse_f32(fields[11], line_no + 1, "biome_tint")?
                            .clamp(0.0, 1.25),
                        grain_strength: parse_f32(fields[12], line_no + 1, "grain_strength")?
                            .clamp(0.0, 1.0),
                    };
                    book.set_style(block, style);
                }
                _ => {}
            }
        }
        Ok(book)
    }

    pub fn style(&self, block: Block) -> BlockStyle {
        self.styles[block.to_id() as usize]
    }

    pub fn set_style(&mut self, block: Block, mut style: BlockStyle) {
        style.top = clamp_color(style.top);
        style.side = clamp_color(style.side);
        style.bottom = clamp_color(style.bottom);
        style.biome_tint_strength = style.biome_tint_strength.clamp(0.0, 1.25);
        style.grain_strength = style.grain_strength.clamp(0.0, 1.0);
        self.styles[block.to_id() as usize] = style;
    }

    pub fn adjust_face_channel(
        &mut self,
        block: Block,
        face: TextureFace,
        channel: usize,
        delta: f32,
    ) {
        if channel > 2 || delta.abs() <= f32::EPSILON {
            return;
        }
        let mut style = self.style(block);
        let mut color = style.face_color(face);
        color[channel] = (color[channel] + delta).clamp(0.0, 1.0);
        style.set_face_color(face, color);
        self.set_style(block, style);
    }

    pub fn adjust_biome_tint_strength(&mut self, block: Block, delta: f32) {
        if delta.abs() <= f32::EPSILON {
            return;
        }
        let mut style = self.style(block);
        style.biome_tint_strength = (style.biome_tint_strength + delta).clamp(0.0, 1.25);
        self.set_style(block, style);
    }

    pub fn adjust_grain_strength(&mut self, block: Block, delta: f32) {
        if delta.abs() <= f32::EPSILON {
            return;
        }
        let mut style = self.style(block);
        style.grain_strength = (style.grain_strength + delta).clamp(0.0, 1.0);
        self.set_style(block, style);
    }

    pub fn apply_brush(
        &mut self,
        block: Block,
        face: TextureFace,
        brush: TextureBrush,
        strength: f32,
        noise_seed: u64,
    ) {
        if strength.abs() <= f32::EPSILON {
            return;
        }
        let mut style = self.style(block);
        if brush == TextureBrush::Shade {
            let base = style.face_color(face);
            let side_factor = (1.0 - strength * 0.42).clamp(0.2, 1.6);
            let bottom_factor = (1.0 - strength * 0.66).clamp(0.15, 1.4);
            style.top = base;
            style.side = mul_color(base, side_factor);
            style.bottom = mul_color(base, bottom_factor);
            self.set_style(block, style);
            return;
        }

        let mut color = style.face_color(face);
        color = apply_color_brush(color, brush, strength, noise_seed);
        style.set_face_color(face, color);
        self.set_style(block, style);
    }
}

fn style_for_block(block: Block) -> BlockStyle {
    match block {
        Block::Air => BlockStyle {
            top: [0.0, 0.0, 0.0],
            side: [0.0, 0.0, 0.0],
            bottom: [0.0, 0.0, 0.0],
            biome_tint_strength: 0.0,
            grain_strength: 0.0,
        },
        Block::Water => BlockStyle {
            top: [0.22, 0.46, 0.78],
            side: [0.18, 0.36, 0.64],
            bottom: [0.12, 0.24, 0.44],
            biome_tint_strength: 1.0,
            grain_strength: 0.05,
        },
        Block::Grass => BlockStyle {
            top: [0.36, 0.78, 0.40],
            side: [0.33, 0.25, 0.16],
            bottom: [0.16, 0.22, 0.14],
            biome_tint_strength: 1.0,
            grain_strength: 0.16,
        },
        Block::Dirt => BlockStyle {
            top: [0.40, 0.28, 0.18],
            side: [0.35, 0.23, 0.14],
            bottom: [0.22, 0.14, 0.09],
            biome_tint_strength: 1.0,
            grain_strength: 0.12,
        },
        Block::Stone => BlockStyle {
            top: [0.55, 0.57, 0.60],
            side: [0.48, 0.50, 0.53],
            bottom: [0.35, 0.36, 0.39],
            biome_tint_strength: 0.0,
            grain_strength: 0.28,
        },
        Block::Deepslate => BlockStyle {
            top: [0.34, 0.36, 0.39],
            side: [0.28, 0.30, 0.33],
            bottom: [0.22, 0.24, 0.27],
            biome_tint_strength: 0.0,
            grain_strength: 0.22,
        },
        Block::DeepDark => BlockStyle {
            top: [0.06, 0.08, 0.09],
            side: [0.04, 0.06, 0.07],
            bottom: [0.02, 0.03, 0.04],
            biome_tint_strength: 0.0,
            grain_strength: 0.18,
        },
        Block::CustomA => BlockStyle {
            top: [0.82, 0.42, 0.28],
            side: [0.70, 0.30, 0.19],
            bottom: [0.44, 0.19, 0.11],
            biome_tint_strength: 0.08,
            grain_strength: 0.10,
        },
        Block::CustomB => BlockStyle {
            top: [0.28, 0.55, 0.89],
            side: [0.20, 0.42, 0.71],
            bottom: [0.12, 0.24, 0.45],
            biome_tint_strength: 0.06,
            grain_strength: 0.14,
        },
        Block::CustomC => BlockStyle {
            top: [0.75, 0.77, 0.33],
            side: [0.62, 0.64, 0.26],
            bottom: [0.42, 0.44, 0.18],
            biome_tint_strength: 0.10,
            grain_strength: 0.12,
        },
    }
}

fn block_key(block: Block) -> &'static str {
    match block {
        Block::Air => "air",
        Block::Water => "water",
        Block::Grass => "grass",
        Block::Dirt => "dirt",
        Block::Stone => "stone",
        Block::Deepslate => "deepslate",
        Block::DeepDark => "deep_dark",
        Block::CustomA => "custom_a",
        Block::CustomB => "custom_b",
        Block::CustomC => "custom_c",
    }
}

pub fn block_from_key(key: &str) -> Option<Block> {
    match key.trim().to_ascii_lowercase().as_str() {
        "air" => Some(Block::Air),
        "water" => Some(Block::Water),
        "grass" => Some(Block::Grass),
        "dirt" => Some(Block::Dirt),
        "stone" => Some(Block::Stone),
        "deepslate" => Some(Block::Deepslate),
        "deep_dark" | "deepdark" => Some(Block::DeepDark),
        "custom_a" | "customa" => Some(Block::CustomA),
        "custom_b" | "customb" => Some(Block::CustomB),
        "custom_c" | "customc" => Some(Block::CustomC),
        _ => None,
    }
}

fn parse_f32(input: &str, line_no: usize, field: &str) -> Result<f32, String> {
    input
        .parse::<f32>()
        .map_err(|err| format!("line {line_no}: invalid {field} `{input}`: {err}"))
}

fn clamp_color(color: Color) -> Color {
    [
        color[0].clamp(0.0, 1.0),
        color[1].clamp(0.0, 1.0),
        color[2].clamp(0.0, 1.0),
    ]
}

fn mul_color(color: Color, factor: f32) -> Color {
    [
        (color[0] * factor).clamp(0.0, 1.0),
        (color[1] * factor).clamp(0.0, 1.0),
        (color[2] * factor).clamp(0.0, 1.0),
    ]
}

fn apply_color_brush(color: Color, brush: TextureBrush, strength: f32, seed: u64) -> Color {
    let amount = strength.clamp(-1.0, 1.0);
    let mut out = color;
    match brush {
        TextureBrush::Lighten => {
            out = [
                (out[0] + amount).clamp(0.0, 1.0),
                (out[1] + amount).clamp(0.0, 1.0),
                (out[2] + amount).clamp(0.0, 1.0),
            ];
        }
        TextureBrush::Darken => {
            out = [
                (out[0] - amount).clamp(0.0, 1.0),
                (out[1] - amount).clamp(0.0, 1.0),
                (out[2] - amount).clamp(0.0, 1.0),
            ];
        }
        TextureBrush::Saturate => {
            let luma = out[0] * 0.2126 + out[1] * 0.7152 + out[2] * 0.0722;
            let sat = 1.0 + amount * 1.4;
            out = [
                (luma + (out[0] - luma) * sat).clamp(0.0, 1.0),
                (luma + (out[1] - luma) * sat).clamp(0.0, 1.0),
                (luma + (out[2] - luma) * sat).clamp(0.0, 1.0),
            ];
        }
        TextureBrush::Desaturate => {
            let luma = out[0] * 0.2126 + out[1] * 0.7152 + out[2] * 0.0722;
            let t = amount.abs().clamp(0.0, 1.0);
            out = [
                out[0] + (luma - out[0]) * t,
                out[1] + (luma - out[1]) * t,
                out[2] + (luma - out[2]) * t,
            ];
        }
        TextureBrush::Warm => {
            out = [
                (out[0] + amount * 0.75).clamp(0.0, 1.0),
                (out[1] + amount * 0.18).clamp(0.0, 1.0),
                (out[2] - amount * 0.52).clamp(0.0, 1.0),
            ];
        }
        TextureBrush::Cool => {
            out = [
                (out[0] - amount * 0.52).clamp(0.0, 1.0),
                (out[1] + amount * 0.12).clamp(0.0, 1.0),
                (out[2] + amount * 0.75).clamp(0.0, 1.0),
            ];
        }
        TextureBrush::Noise => {
            for (channel, value) in out.iter_mut().enumerate() {
                let jitter = (noise_unit(seed.wrapping_add(channel as u64 * 101)) - 0.5)
                    * 2.0
                    * amount.abs();
                *value = (*value + jitter).clamp(0.0, 1.0);
            }
        }
        TextureBrush::Shade => {}
    }
    out
}

fn noise_unit(mut x: u64) -> f32 {
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    let mixed = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
    (mixed as f32 / u64::MAX as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_book_round_trip_stable() {
        let book = BlockStyleBook::default();
        let text = book.to_text();
        let parsed = BlockStyleBook::from_text(&text).expect("style parse");
        assert_eq!(book, parsed);
    }

    #[test]
    fn shade_brush_rebuilds_side_and_bottom_from_selected_face() {
        let mut book = BlockStyleBook::default();
        let before = book.style(Block::CustomA);
        book.apply_brush(
            Block::CustomA,
            TextureFace::Top,
            TextureBrush::Shade,
            0.45,
            7,
        );
        let after = book.style(Block::CustomA);
        assert_ne!(after.side, before.side);
        assert_ne!(after.bottom, before.bottom);
        assert_eq!(after.top, before.top);
    }
}
