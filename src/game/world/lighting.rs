use super::*;

pub(super) fn build_light_volume(
    block_ids: &[u8],
    xz_extent: usize,
    y_extent: usize,
    min_y: usize,
    max_y: usize,
) -> Vec<u8> {
    let layer_stride = xz_extent * xz_extent;
    let grid_index = |x: usize, y: usize, z: usize| y * layer_stride + z * xz_extent + x;
    let mut light_levels = vec![15_u8; block_ids.len()];

    for (idx, block_id) in block_ids.iter().enumerate() {
        if Block::from_id(*block_id).is_solid() {
            light_levels[idx] = 0;
        }
    }

    if y_extent == 0 {
        return light_levels;
    }

    let band_min = min_y.min(y_extent - 1);
    let band_max = max_y.min(y_extent - 1).max(band_min);

    for z in 0..xz_extent {
        for x in 0..xz_extent {
            let mut first_solid_from_top = None;
            for y in (band_min..=band_max).rev() {
                let idx = grid_index(x, y, z);
                let block = Block::from_id(block_ids[idx]);
                if block.is_solid() {
                    first_solid_from_top = Some(y);
                    break;
                }
            }
            let ceiling = first_solid_from_top.unwrap_or(band_min);
            for y in band_min..=band_max {
                let idx = grid_index(x, y, z);
                let block = Block::from_id(block_ids[idx]);
                if block.is_solid() {
                    continue;
                }
                let level = if first_solid_from_top.is_none() || y > ceiling {
                    15
                } else {
                    let depth = (ceiling + 1).saturating_sub(y) as u8;
                    15_u8.saturating_sub(depth)
                };
                light_levels[idx] = level;
            }
        }
    }

    // Hook for emissive blocks: inject nearby light without a full flood-fill.
    for z in 0..xz_extent {
        for x in 0..xz_extent {
            for y in band_min..=band_max {
                let idx = grid_index(x, y, z);
                let block = Block::from_id(block_ids[idx]);
                if !block.is_solid() {
                    continue;
                }
                let emission = block_light_emission(block);
                if emission == 0 {
                    continue;
                }
                if x > 0 {
                    let n = grid_index(x - 1, y, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if x + 1 < xz_extent {
                    let n = grid_index(x + 1, y, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if y > 0 {
                    let n = grid_index(x, y - 1, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if y + 1 < y_extent {
                    let n = grid_index(x, y + 1, z);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if z > 0 {
                    let n = grid_index(x, y, z - 1);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
                if z + 1 < xz_extent {
                    let n = grid_index(x, y, z + 1);
                    if !Block::from_id(block_ids[n]).is_solid() {
                        light_levels[n] = light_levels[n].max(emission.saturating_sub(1));
                    }
                }
            }
        }
    }

    light_levels
}

pub(super) fn sample_light<F>(
    light_levels: &[u8],
    grid_index: &F,
    xz_extent: usize,
    y_extent: usize,
    x: usize,
    y: usize,
    z: usize,
) -> u8
where
    F: Fn(usize, usize, usize) -> usize,
{
    if x >= xz_extent || z >= xz_extent {
        return 15;
    }
    if y >= y_extent {
        return 15;
    }
    light_levels[grid_index(x, y, z)]
}

pub(super) fn apply_light(color: [f32; 3], light_level: u8, face_shade: f32) -> [f32; 3] {
    let block_light = (light_level as f32 / 15.0).clamp(0.0, 1.0);
    // Keep side faces from collapsing into near-black in shaded regions.
    let min_floor = if face_shade < 0.9 { 0.16 } else { 0.10 };
    let brightness = ((0.14 + block_light * 0.86) * face_shade).clamp(min_floor, 1.1);
    [
        (color[0] * brightness).clamp(0.0, 1.0),
        (color[1] * brightness).clamp(0.0, 1.0),
        (color[2] * brightness).clamp(0.0, 1.0),
    ]
}

fn block_light_emission(block: Block) -> u8 {
    block.properties().light_emission
}
