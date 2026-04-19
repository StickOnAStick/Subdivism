use super::*;

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

const GRASS_TOP_A: Color = [0.36, 0.78, 0.40];
const GRASS_TOP_B: Color = [0.30, 0.70, 0.34];
const GRASS_BASE: FacePalette = FacePalette::new(
    GRASS_TOP_A,        // top is selected per checker variant
    [0.33, 0.25, 0.16], // sides (dirt + grass fringe)
    [0.16, 0.22, 0.14], // underside
);

const DIRT_BASE: FacePalette = FacePalette::new(
    [0.40, 0.28, 0.18], // top
    [0.35, 0.23, 0.14], // sides
    [0.22, 0.14, 0.09], // underside
);

const STONE_BASE: FacePalette = FacePalette::new(
    [0.55, 0.57, 0.60], // top
    [0.48, 0.50, 0.53], // sides
    [0.35, 0.36, 0.39], // underside
);

const DEEPSLATE_BASE: FacePalette = FacePalette::new(
    [0.34, 0.36, 0.39], // top
    [0.28, 0.30, 0.33], // sides
    [0.22, 0.24, 0.27], // underside
);

const DEEP_DARK_BASE: FacePalette = FacePalette::new(
    [0.06, 0.08, 0.09], // top
    [0.04, 0.06, 0.07], // sides
    [0.02, 0.03, 0.04], // underside
);

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

pub(super) fn palette(block: Block, x: i64, z: i64, biome: BiomeKind) -> (Color, Color, Color) {
    let tint = biome_tint(biome);
    match block {
        Block::Water => tint_palette(
            FacePalette::new([0.22, 0.46, 0.78], [0.18, 0.36, 0.64], [0.12, 0.24, 0.44]),
            tint,
        )
        .to_tuple(),
        Block::Grass => {
            let top = if ((x + z) & 1) == 0 {
                GRASS_TOP_A
            } else {
                GRASS_TOP_B
            };
            tint_palette(
                FacePalette::new(top, GRASS_BASE.side, GRASS_BASE.bottom),
                tint,
            )
            .to_tuple()
        }
        Block::Dirt => tint_palette(DIRT_BASE, tint).to_tuple(),
        // Keep rock tones biome-neutral so stone never reads as sand/mud from biome tinting.
        Block::Stone => {
            let offset = stone_variation_offset(x, z);
            apply_uniform_brightness_offset(STONE_BASE, offset).to_tuple()
        }
        Block::Deepslate => DEEPSLATE_BASE.to_tuple(),
        Block::DeepDark => DEEP_DARK_BASE.to_tuple(),
        Block::Air => AIR_BASE.to_tuple(),
    }
}
