use std::collections::HashMap;

use glam::Vec3;

use super::*;
use super::lighting::{apply_light, build_light_volume, sample_light};

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
                column_top[column_index(local_x, local_z)] = clamped_top;
                min_surface = min_surface.min(clamped_top);
                max_surface = max_surface.max(clamped_top);
            }
        }

        let absolute_depth_floor = WORLD_RENDER_FLOOR_Y.max(WORLD_MIN_Y);
        let mut min_fill_y = absolute_depth_floor;
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
                let surface_y = self.surface_height(world_x, world_z);
                let clamped_top = surface_y.clamp(WORLD_MIN_Y, WORLD_MAX_Y).min(max_fill_y);
                for y in min_fill_y..=clamped_top {
                    let Some(yi) = local_y_index(y) else {
                        continue;
                    };
                    let block = self.procedural_block_with_surface(world_x, y, world_z, surface_y);
                    block_ids[grid_index(local_x, yi, local_z)] = block.to_id();
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

        let light_levels = build_light_volume(&block_ids, xz_extent, y_extent);

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
                    let (top_color, side_color, bottom_color) =
                        self.palette_at(block, world_x, world_z);

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
                            apply_light(top_color, light, 1.0),
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
                            apply_light(bottom_color, light, 0.62),
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
                            apply_light(side_color, light, 0.78),
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
                            apply_light(side_color, light, 0.84),
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
                            apply_light(side_color, light, 0.76),
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
                            apply_light(side_color, light, 0.82),
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
