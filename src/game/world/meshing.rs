use std::collections::HashMap;

use glam::Vec3;

use super::lighting::{apply_light, build_light_volume, sample_light};
use super::palette::palette;
use super::*;

fn face_noise_unit(x: i64, y: i32, z: i64, salt: u64) -> f32 {
    let mut h = salt;
    h ^= (x as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    h ^= (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h as f32 / u64::MAX as f32).clamp(0.0, 1.0)
}

fn vary_face_color(base: [f32; 3], block: Block, x: i64, y: i32, z: i64, face_id: u64) -> [f32; 3] {
    let grain = 0.90 + face_noise_unit(x, y, z, 0xA54F_F53A ^ face_id) * 0.20;
    let mut shade = grain;
    if matches!(block, Block::Dirt | Block::Grass) {
        let strata =
            0.90 + (((y as f32 * 0.12) + (x as f32 * 0.04) + (z as f32 * 0.03)).sin() * 0.10);
        shade *= strata;
    }
    if matches!(block, Block::Stone | Block::Deepslate | Block::DeepDark) {
        let rocky = 0.94 + face_noise_unit(x, y, z, 0x510E_527F ^ (face_id << 1)) * 0.10;
        shade *= rocky;
    }
    [
        (base[0] * shade).clamp(0.0, 1.0),
        (base[1] * shade).clamp(0.0, 1.0),
        (base[2] * shade).clamp(0.0, 1.0),
    ]
}

impl World {
    pub fn build_chunk_mesh(&self, chunk: (i64, i64)) -> Vec<Vertex> {
        let overrides = self
            .storage
            .overrides
            .read()
            .expect("overrides read lock poisoned");
        self.build_chunk_mesh_with_overrides(chunk, &overrides)
    }

    pub fn build_chunk_mesh_lod(&self, origin_chunk: (i64, i64), lod_level: u8) -> Vec<Vertex> {
        if lod_level == 0 {
            return self.build_chunk_mesh(origin_chunk);
        }

        let chunk_span = 1_i64 << lod_level;
        let tile_size_blocks = CHUNK_SIZE * chunk_span;
        let sample_blocks = 2_i64 << lod_level;
        let cells = tile_size_blocks / sample_blocks;
        let base_x = origin_chunk.0 * CHUNK_SIZE;
        let base_z = origin_chunk.1 * CHUNK_SIZE;
        let mut vertices = Vec::new();

        let mut sampled_heights = vec![0.0_f32; ((cells + 1) * (cells + 1)) as usize];
        let height_at =
            |heights: &[f32], x: i64, z: i64| -> f32 { heights[(z * (cells + 1) + x) as usize] };
        let set_height = |heights: &mut [f32], x: i64, z: i64, value: f32| {
            let index = (z * (cells + 1) + x) as usize;
            heights[index] = value;
        };

        for gz in 0..=cells {
            for gx in 0..=cells {
                let sample_x = base_x + gx * sample_blocks + sample_blocks / 2;
                let sample_z = base_z + gz * sample_blocks + sample_blocks / 2;
                let top_y = self.surface_height(sample_x, sample_z) as f32 + 1.0;
                set_height(&mut sampled_heights, gx, gz, top_y);
            }
        }

        for gz in 0..cells {
            for gx in 0..cells {
                let wx0 = base_x + gx * sample_blocks;
                let wz0 = base_z + gz * sample_blocks;
                let wx1 = wx0 + sample_blocks;
                let wz1 = wz0 + sample_blocks;

                let center_x = wx0 + sample_blocks / 2;
                let center_z = wz0 + sample_blocks / 2;
                let top_y = height_at(&sampled_heights, gx, gz);
                let east_y = height_at(&sampled_heights, gx + 1, gz);
                let south_y = height_at(&sampled_heights, gx, gz + 1);
                let block = self.procedural_block(center_x, top_y as i32 - 1, center_z);
                let (top_color, side_color, _) = self.palette_at(block, center_x, center_z);

                push_quad(
                    &mut vertices,
                    [
                        Vec3::new(wx0 as f32, top_y, wz0 as f32),
                        Vec3::new(wx0 as f32, top_y, wz1 as f32),
                        Vec3::new(wx1 as f32, top_y, wz1 as f32),
                        Vec3::new(wx1 as f32, top_y, wz0 as f32),
                    ],
                    top_color,
                    Vec3::Y,
                );

                if (top_y - east_y).abs() > 0.01 {
                    let low = top_y.min(east_y);
                    let high = top_y.max(east_y);
                    if top_y > east_y {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, low, wz0 as f32),
                                Vec3::new(wx1 as f32, high, wz0 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                            ],
                            side_color,
                            Vec3::X,
                        );
                    } else {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx1 as f32, low, wz0 as f32),
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz0 as f32),
                            ],
                            side_color,
                            -Vec3::X,
                        );
                    }
                }

                if (top_y - south_y).abs() > 0.01 {
                    let low = top_y.min(south_y);
                    let high = top_y.max(south_y);
                    if top_y > south_y {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx0 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                                Vec3::new(wx0 as f32, high, wz1 as f32),
                            ],
                            side_color,
                            Vec3::Z,
                        );
                    } else {
                        push_quad(
                            &mut vertices,
                            [
                                Vec3::new(wx1 as f32, low, wz1 as f32),
                                Vec3::new(wx0 as f32, low, wz1 as f32),
                                Vec3::new(wx0 as f32, high, wz1 as f32),
                                Vec3::new(wx1 as f32, high, wz1 as f32),
                            ],
                            side_color,
                            -Vec3::Z,
                        );
                    }
                }
            }
        }

        let boundary_height = |sample_x: i64, sample_z: i64| -> f32 {
            self.surface_height(sample_x, sample_z) as f32 + 1.0
        };

        for gz in 0..cells {
            let wz0 = base_z + gz * sample_blocks;
            let wz1 = wz0 + sample_blocks;
            let sample_z = wz0 + sample_blocks / 2;

            let west_inside = height_at(&sampled_heights, 0, gz);
            let west_outside = boundary_height(base_x - sample_blocks / 2, sample_z);
            let center_z = wz0 + sample_blocks / 2;
            let west_block = self.procedural_block(base_x, west_inside as i32 - 1, center_z);
            let (_, west_side_color, _) = self.palette_at(west_block, base_x, center_z);
            if (west_inside - west_outside).abs() > 0.01 {
                let low = west_inside.min(west_outside);
                let high = west_inside.max(west_outside);
                if west_inside > west_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(base_x as f32, low, wz0 as f32),
                            Vec3::new(base_x as f32, low, wz1 as f32),
                            Vec3::new(base_x as f32, high, wz1 as f32),
                            Vec3::new(base_x as f32, high, wz0 as f32),
                        ],
                        west_side_color,
                        -Vec3::X,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(base_x as f32, low, wz1 as f32),
                            Vec3::new(base_x as f32, low, wz0 as f32),
                            Vec3::new(base_x as f32, high, wz0 as f32),
                            Vec3::new(base_x as f32, high, wz1 as f32),
                        ],
                        west_side_color,
                        Vec3::X,
                    );
                }
            }

            let east_x = base_x + tile_size_blocks;
            let east_inside = height_at(&sampled_heights, cells - 1, gz);
            let east_outside = boundary_height(east_x + sample_blocks / 2, sample_z);
            let east_block = self.procedural_block(east_x - 1, east_inside as i32 - 1, center_z);
            let (_, east_side_color, _) = self.palette_at(east_block, east_x - 1, center_z);
            if (east_inside - east_outside).abs() > 0.01 {
                let low = east_inside.min(east_outside);
                let high = east_inside.max(east_outside);
                if east_inside > east_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(east_x as f32, low, wz1 as f32),
                            Vec3::new(east_x as f32, low, wz0 as f32),
                            Vec3::new(east_x as f32, high, wz0 as f32),
                            Vec3::new(east_x as f32, high, wz1 as f32),
                        ],
                        east_side_color,
                        Vec3::X,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(east_x as f32, low, wz0 as f32),
                            Vec3::new(east_x as f32, low, wz1 as f32),
                            Vec3::new(east_x as f32, high, wz1 as f32),
                            Vec3::new(east_x as f32, high, wz0 as f32),
                        ],
                        east_side_color,
                        -Vec3::X,
                    );
                }
            }
        }

        for gx in 0..cells {
            let wx0 = base_x + gx * sample_blocks;
            let wx1 = wx0 + sample_blocks;
            let sample_x = wx0 + sample_blocks / 2;

            let north_inside = height_at(&sampled_heights, gx, 0);
            let north_outside = boundary_height(sample_x, base_z - sample_blocks / 2);
            let center_x = wx0 + sample_blocks / 2;
            let north_block = self.procedural_block(center_x, north_inside as i32 - 1, base_z);
            let (_, north_side_color, _) = self.palette_at(north_block, center_x, base_z);
            if (north_inside - north_outside).abs() > 0.01 {
                let low = north_inside.min(north_outside);
                let high = north_inside.max(north_outside);
                if north_inside > north_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx1 as f32, low, base_z as f32),
                            Vec3::new(wx0 as f32, low, base_z as f32),
                            Vec3::new(wx0 as f32, high, base_z as f32),
                            Vec3::new(wx1 as f32, high, base_z as f32),
                        ],
                        north_side_color,
                        -Vec3::Z,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx0 as f32, low, base_z as f32),
                            Vec3::new(wx1 as f32, low, base_z as f32),
                            Vec3::new(wx1 as f32, high, base_z as f32),
                            Vec3::new(wx0 as f32, high, base_z as f32),
                        ],
                        north_side_color,
                        Vec3::Z,
                    );
                }
            }

            let south_z = base_z + tile_size_blocks;
            let south_inside = height_at(&sampled_heights, gx, cells - 1);
            let south_outside = boundary_height(sample_x, south_z + sample_blocks / 2);
            let south_block = self.procedural_block(center_x, south_inside as i32 - 1, south_z - 1);
            let (_, south_side_color, _) = self.palette_at(south_block, center_x, south_z - 1);
            if (south_inside - south_outside).abs() > 0.01 {
                let low = south_inside.min(south_outside);
                let high = south_inside.max(south_outside);
                if south_inside > south_outside {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx0 as f32, low, south_z as f32),
                            Vec3::new(wx1 as f32, low, south_z as f32),
                            Vec3::new(wx1 as f32, high, south_z as f32),
                            Vec3::new(wx0 as f32, high, south_z as f32),
                        ],
                        south_side_color,
                        Vec3::Z,
                    );
                } else {
                    push_quad(
                        &mut vertices,
                        [
                            Vec3::new(wx1 as f32, low, south_z as f32),
                            Vec3::new(wx0 as f32, low, south_z as f32),
                            Vec3::new(wx0 as f32, high, south_z as f32),
                            Vec3::new(wx1 as f32, high, south_z as f32),
                        ],
                        south_side_color,
                        -Vec3::Z,
                    );
                }
            }
        }

        vertices
    }

    fn build_chunk_mesh_with_overrides(
        &self,
        chunk: (i64, i64),
        overrides: &HashMap<BlockPos, Block>,
    ) -> Vec<Vertex> {
        let base_x = chunk.0 * CHUNK_SIZE;
        let base_z = chunk.1 * CHUNK_SIZE;
        let xz_extent = (CHUNK_SIZE + 2) as usize;
        let layer_stride = xz_extent * xz_extent;
        let mut column_top = vec![WORLD_MIN_Y; layer_stride];
        let mut surface_heights = vec![WORLD_MIN_Y; layer_stride];
        let mut column_biomes = vec![BiomeKind::Meadow; layer_stride];
        let mut min_surface = WORLD_MAX_Y;
        let mut max_surface = WORLD_MIN_Y;
        let min_x = base_x - 1;
        let max_x = base_x + CHUNK_SIZE;
        let min_z = base_z - 1;
        let max_z = base_z + CHUNK_SIZE;
        let column_index = |x: usize, z: usize| z * xz_extent + x;

        for local_z in 0..xz_extent {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 0..xz_extent {
                let world_x = base_x + local_x as i64 - 1;
                let surface_y = self.surface_height(world_x, world_z);
                let clamped_top = surface_y.clamp(WORLD_MIN_Y, WORLD_MAX_Y);
                let column = column_index(local_x, local_z);
                surface_heights[column] = clamped_top;
                column_top[column] = clamped_top;
                column_biomes[column] = self.biome_at(world_x, world_z);
                min_surface = min_surface.min(clamped_top);
                max_surface = max_surface.max(clamped_top);
            }
        }

        let absolute_depth_floor = WORLD_RENDER_FLOOR_Y.max(WORLD_MIN_Y);
        let terrain_depth_floor =
            (min_surface - MAX_NATURAL_CAVE_DEPTH_BELOW_SURFACE - 2).max(absolute_depth_floor);
        let mut min_fill_y = terrain_depth_floor;
        let mut max_fill_y = max_surface.min(WORLD_MAX_Y);
        if !overrides.is_empty() {
            for (pos, block) in overrides {
                if !contains_world_y(pos.y) {
                    continue;
                }
                if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
                    continue;
                }
                min_fill_y = min_fill_y.min((pos.y - 1).max(WORLD_MIN_Y));
                if block.is_solid() {
                    max_fill_y = max_fill_y.max(pos.y.min(WORLD_MAX_Y));
                }
            }
        }
        if max_fill_y < min_fill_y {
            return Vec::new();
        }

        let y_extent = (max_fill_y - min_fill_y + 1) as usize;
        let mut block_ids = vec![Block::Air.to_id(); layer_stride * y_extent];
        let grid_index = |x: usize, y: usize, z: usize| y * layer_stride + z * xz_extent + x;
        let local_y_index = |world_y: i32| -> Option<usize> {
            if world_y < min_fill_y || world_y > max_fill_y {
                None
            } else {
                Some((world_y - min_fill_y) as usize)
            }
        };

        for local_z in 0..xz_extent {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 0..xz_extent {
                let world_x = base_x + local_x as i64 - 1;
                let surface_y = surface_heights[column_index(local_x, local_z)];
                let clamped_top = surface_y.min(max_fill_y);
                if clamped_top < min_fill_y {
                    continue;
                }

                let cave_floor_y =
                    (surface_y - MAX_NATURAL_CAVE_DEPTH_BELOW_SURFACE).max(min_fill_y);
                let deep_end = (cave_floor_y - 1).min(clamped_top);
                if deep_end >= min_fill_y {
                    let deep_dark_end = deep_end.min(-129);
                    if deep_dark_end >= min_fill_y {
                        for y in min_fill_y..=deep_dark_end {
                            if let Some(yi) = local_y_index(y) {
                                block_ids[grid_index(local_x, yi, local_z)] =
                                    Block::DeepDark.to_id();
                            }
                        }
                    }

                    let deepslate_start = min_fill_y.max(-128);
                    let deepslate_end = deep_end.min(-1);
                    if deepslate_end >= deepslate_start {
                        for y in deepslate_start..=deepslate_end {
                            if let Some(yi) = local_y_index(y) {
                                block_ids[grid_index(local_x, yi, local_z)] =
                                    Block::Deepslate.to_id();
                            }
                        }
                    }

                    let stone_start = min_fill_y.max(0);
                    if deep_end >= stone_start {
                        for y in stone_start..=deep_end {
                            if let Some(yi) = local_y_index(y) {
                                block_ids[grid_index(local_x, yi, local_z)] = Block::Stone.to_id();
                            }
                        }
                    }
                }

                let cave_band_start = cave_floor_y.max(min_fill_y);
                if clamped_top >= cave_band_start {
                    for y in cave_band_start..=clamped_top {
                        let Some(yi) = local_y_index(y) else {
                            continue;
                        };
                        let block = if should_carve_air(self.seed, world_x, y, world_z, surface_y) {
                            Block::Air
                        } else if y == surface_y && y >= WORLD_OVERWORLD_FLOOR {
                            Block::Grass
                        } else if y >= surface_y - 2 && y >= WORLD_OVERWORLD_FLOOR - 16 {
                            Block::Dirt
                        } else {
                            crust_block_for_y(y)
                        };
                        block_ids[grid_index(local_x, yi, local_z)] = block.to_id();
                    }
                }
            }
        }

        if !overrides.is_empty() {
            for (pos, block) in overrides {
                if !contains_world_y(pos.y) {
                    continue;
                }
                if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
                    continue;
                }
                let local_x = (pos.x - base_x + 1) as usize;
                let local_z = (pos.z - base_z + 1) as usize;
                let Some(y) = local_y_index(pos.y) else {
                    continue;
                };
                block_ids[grid_index(local_x, y, local_z)] = block.to_id();
                if block.is_solid() {
                    let top = &mut column_top[column_index(local_x, local_z)];
                    if pos.y > *top {
                        *top = pos.y;
                    }
                }
            }
        }

        let mut light_min_y = 0_usize;
        let mut light_max_y = y_extent.saturating_sub(1);
        let mut found_air_slice = false;
        for yi in 0..y_extent {
            let slice_start = yi * layer_stride;
            let slice_end = slice_start + layer_stride;
            let has_air = block_ids[slice_start..slice_end]
                .iter()
                .any(|id| !Block::from_id(*id).is_solid());
            if !has_air {
                continue;
            }
            if !found_air_slice {
                light_min_y = yi;
                light_max_y = yi;
                found_air_slice = true;
            } else {
                light_max_y = yi;
            }
        }
        if found_air_slice {
            light_min_y = light_min_y.saturating_sub(1);
            light_max_y = (light_max_y + 1).min(y_extent.saturating_sub(1));
        } else {
            light_min_y = 0;
            light_max_y = y_extent.saturating_sub(1);
        }
        let light_levels =
            build_light_volume(&block_ids, xz_extent, y_extent, light_min_y, light_max_y);

        let mut vertices = Vec::with_capacity(24_576);
        for local_z in 1..=CHUNK_SIZE as usize {
            let world_z = base_z + local_z as i64 - 1;
            for local_x in 1..=CHUNK_SIZE as usize {
                let world_x = base_x + local_x as i64 - 1;
                let top = column_top[column_index(local_x, local_z)].min(max_fill_y);
                for y in min_fill_y..=top {
                    let Some(yi) = local_y_index(y) else {
                        continue;
                    };
                    let block_id = block_ids[grid_index(local_x, yi, local_z)];
                    if block_id == Block::Air.to_id() {
                        continue;
                    }
                    let block = Block::from_id(block_id);
                    let base = Vec3::new(world_x as f32, y as f32, world_z as f32);
                    let biome = column_biomes[column_index(local_x, local_z)];
                    let (top_color, side_color, bottom_color) =
                        palette(block, world_x, world_z, biome);

                    if yi + 1 >= y_extent
                        || block_ids[grid_index(local_x, yi + 1, local_z)] == Block::Air.to_id()
                    {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi + 1,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            apply_light(
                                vary_face_color(top_color, block, world_x, y, world_z, 11),
                                light,
                                1.0,
                            ),
                            Vec3::Y,
                        );
                    }
                    if yi > 0
                        && block_ids[grid_index(local_x, yi - 1, local_z)] == Block::Air.to_id()
                    {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi - 1,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                            ],
                            apply_light(
                                vary_face_color(bottom_color, block, world_x, y, world_z, 22),
                                light,
                                0.62,
                            ),
                            -Vec3::Y,
                        );
                    }
                    if block_ids[grid_index(local_x - 1, yi, local_z)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x - 1,
                            yi,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                            ],
                            apply_light(
                                vary_face_color(side_color, block, world_x, y, world_z, 31),
                                light,
                                0.78,
                            ),
                            -Vec3::X,
                        );
                    }
                    if block_ids[grid_index(local_x + 1, yi, local_z)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x + 1,
                            yi,
                            local_z,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                            ],
                            apply_light(
                                vary_face_color(side_color, block, world_x, y, world_z, 32),
                                light,
                                0.84,
                            ),
                            Vec3::X,
                        );
                    }
                    if block_ids[grid_index(local_x, yi, local_z - 1)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi,
                            local_z - 1,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            apply_light(
                                vary_face_color(side_color, block, world_x, y, world_z, 33),
                                light,
                                0.76,
                            ),
                            -Vec3::Z,
                        );
                    }
                    if block_ids[grid_index(local_x, yi, local_z + 1)] == Block::Air.to_id() {
                        let light = sample_light(
                            &light_levels,
                            &grid_index,
                            xz_extent,
                            y_extent,
                            local_x,
                            yi,
                            local_z + 1,
                        );
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                            ],
                            apply_light(
                                vary_face_color(side_color, block, world_x, y, world_z, 34),
                                light,
                                0.82,
                            ),
                            Vec3::Z,
                        );
                    }
                }
            }
        }

        let sub_overrides = self
            .storage
            .sub_overrides
            .read()
            .expect("sub overrides read lock poisoned");
        let chunk_max_x = base_x + CHUNK_SIZE - 1;
        let chunk_max_z = base_z + CHUNK_SIZE - 1;
        for (pos, block) in sub_overrides.iter_all() {
            if !contains_world_y(pos.y)
                || pos.x < base_x
                || pos.x > chunk_max_x
                || pos.z < base_z
                || pos.z > chunk_max_z
                || !block.is_solid()
            {
                continue;
            }
            add_sub_block_cube(&mut vertices, *pos, *block, self);
        }

        vertices
    }
}
