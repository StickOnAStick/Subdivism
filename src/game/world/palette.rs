use super::*;

pub(super) fn palette(block: Block, x: i64, z: i64, biome: BiomeKind) -> ([f32; 3], [f32; 3], [f32; 3]) {
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
        // Keep rock tones biome-neutral so stone never reads as sand/mud from biome tinting.
        Block::Stone => {
            let rock_variation = (((x ^ z) & 3) as f32) * 0.016 - 0.024;
            let top = [
                (0.55 + rock_variation).clamp(0.0, 1.0),
                (0.57 + rock_variation).clamp(0.0, 1.0),
                (0.60 + rock_variation).clamp(0.0, 1.0),
            ];
            let side = [
                (0.48 + rock_variation).clamp(0.0, 1.0),
                (0.50 + rock_variation).clamp(0.0, 1.0),
                (0.53 + rock_variation).clamp(0.0, 1.0),
            ];
            let bottom = [
                (0.35 + rock_variation).clamp(0.0, 1.0),
                (0.36 + rock_variation).clamp(0.0, 1.0),
                (0.39 + rock_variation).clamp(0.0, 1.0),
            ];
            (top, side, bottom)
        }
        Block::Deepslate => ([0.34, 0.36, 0.39], [0.28, 0.30, 0.33], [0.22, 0.24, 0.27]),
        Block::DeepDark => ([0.06, 0.08, 0.09], [0.04, 0.06, 0.07], [0.02, 0.03, 0.04]),
        Block::Air => ([0.0; 3], [0.0; 3], [0.0; 3]),
    }
}
