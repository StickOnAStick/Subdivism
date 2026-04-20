use super::*;
use crate::game::block_style::BlockStyle;

type Color = [f32; 3];

#[derive(Clone, Copy)]
struct FacePalette {
    // Color used for upward-facing quads.
    top: Color,
    // Color used for vertical side quads.
    side: Color,
    // Color used for downward-facing quads.
    bottom: Color,
}

impl FacePalette {
    const fn new(top: Color, side: Color, bottom: Color) -> Self {
        Self { top, side, bottom }
    }

    fn to_tuple(self) -> (Color, Color, Color) {
        (self.top, self.side, self.bottom)
    }
}

const BIOME_TINT_OCEAN: Color = [0.62, 0.82, 1.14];
const BIOME_TINT_MEADOW: Color = [1.00, 1.00, 1.00];
const BIOME_TINT_VALLEY: Color = [0.84, 1.08, 0.84];
const BIOME_TINT_MOUNTAIN: Color = [0.78, 0.84, 0.92];
const BIOME_TINT_DESERT: Color = [1.22, 1.06, 0.70];
const BIOME_TINT_FOREST: Color = [0.70, 1.04, 0.72];

const AIR_BASE: FacePalette = FacePalette::new([0.0; 3], [0.0; 3], [0.0; 3]);

fn tint_color(color: Color, tint: Color) -> Color {
    [
        (color[0] * tint[0]).clamp(0.0, 1.0),
        (color[1] * tint[1]).clamp(0.0, 1.0),
        (color[2] * tint[2]).clamp(0.0, 1.0),
    ]
}

fn tint_palette(base: FacePalette, tint: Color) -> FacePalette {
    FacePalette::new(
        tint_color(base.top, tint),
        tint_color(base.side, tint),
        tint_color(base.bottom, tint),
    )
}

fn blend_tint_strength(tint: Color, strength: f32) -> Color {
    let s = strength.clamp(0.0, 1.25);
    [
        (1.0 + (tint[0] - 1.0) * s).clamp(0.0, 1.35),
        (1.0 + (tint[1] - 1.0) * s).clamp(0.0, 1.35),
        (1.0 + (tint[2] - 1.0) * s).clamp(0.0, 1.35),
    ]
}

fn add_uniform_offset(color: Color, offset: f32) -> Color {
    [
        (color[0] + offset).clamp(0.0, 1.0),
        (color[1] + offset).clamp(0.0, 1.0),
        (color[2] + offset).clamp(0.0, 1.0),
    ]
}

fn apply_uniform_brightness_offset(base: FacePalette, offset: f32) -> FacePalette {
    FacePalette::new(
        add_uniform_offset(base.top, offset),
        add_uniform_offset(base.side, offset),
        add_uniform_offset(base.bottom, offset),
    )
}

fn biome_tint(biome: BiomeKind) -> Color {
    match biome {
        BiomeKind::Ocean => BIOME_TINT_OCEAN,
        BiomeKind::Meadow => BIOME_TINT_MEADOW,
        BiomeKind::Valley => BIOME_TINT_VALLEY,
        BiomeKind::Mountain => BIOME_TINT_MOUNTAIN,
        BiomeKind::Desert => BIOME_TINT_DESERT,
        BiomeKind::Forest => BIOME_TINT_FOREST,
    }
}

fn stone_variation_offset(x: i64, z: i64) -> f32 {
    // Pick a stable 0..=3 bucket from world coordinates.
    let variation_bucket = ((x ^ z) & 0b11) as f32;
    // Map the bucket to [-0.024, -0.008, +0.008, +0.024].
    variation_bucket * 0.016 - 0.024
}

pub(super) fn palette(
    block: Block,
    style: BlockStyle,
    x: i64,
    z: i64,
    biome: BiomeKind,
) -> (Color, Color, Color) {
    let tint = blend_tint_strength(biome_tint(biome), style.biome_tint_strength);
    let mut base = FacePalette::new(style.top, style.side, style.bottom);
    match block {
        Block::Water => tint_palette(base, tint).to_tuple(),
        Block::Grass => {
            // Checker blend keeps grass from appearing as a perfectly flat painted sheet.
            let checker = if ((x + z) & 1) == 0 { 1.0 } else { -1.0 };
            let variation = checker * (0.04 * style.grain_strength.clamp(0.0, 1.0));
            base.top = add_uniform_offset(base.top, variation);
            tint_palette(base, tint).to_tuple()
        }
        Block::Dirt => tint_palette(base, tint).to_tuple(),
        Block::Stone => {
            let offset = stone_variation_offset(x, z) * style.grain_strength.clamp(0.0, 1.0);
            apply_uniform_brightness_offset(base, offset).to_tuple()
        }
        Block::Deepslate | Block::DeepDark => {
            let offset = stone_variation_offset(x, z) * 0.5 * style.grain_strength.clamp(0.0, 1.0);
            apply_uniform_brightness_offset(base, offset).to_tuple()
        }
        Block::CustomA | Block::CustomB | Block::CustomC => {
            let offset = stone_variation_offset(x, z) * 0.35 * style.grain_strength.clamp(0.0, 1.0);
            tint_palette(apply_uniform_brightness_offset(base, offset), tint).to_tuple()
        }
        Block::Air => AIR_BASE.to_tuple(),
    }
}
