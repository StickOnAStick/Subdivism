use super::noise::{fbm_2d, hash2_to_unit, ridge_fbm_2d, smooth_range, value_noise_2d, wave_curve};
use super::palette::palette;
use super::*;

fn legacy_normalized_scale(raw: f32, legacy_min: f32, legacy_max: f32) -> f32 {
    if raw <= legacy_max {
        ((raw - legacy_min) / (legacy_max - legacy_min)).clamp(0.0, 1.0)
    } else {
        raw.clamp(0.0, 1.0)
    }
}

fn span_to_frequency(span: f32, min_frequency: f32, max_frequency: f32) -> f32 {
    let t = span.clamp(0.0, 1.0);
    let curved = t * t * (3.0 - 2.0 * t);
    max_frequency + (min_frequency - max_frequency) * curved
}

fn span_and_frequency(raw: f32, legacy_min: f32, legacy_max: f32) -> (f32, f32) {
    if raw <= legacy_max {
        let frequency = raw.clamp(legacy_min, legacy_max);
        let span = legacy_normalized_scale(frequency, legacy_min, legacy_max);
        (span, frequency)
    } else {
        let span = raw.clamp(0.0, 1.0);
        let frequency = span_to_frequency(span, legacy_min, legacy_max);
        (span, frequency)
    }
}

fn region_frequency_multiplier(region_scale: f32) -> f32 {
    let t = region_scale.clamp(0.0, 1.0);
    let curved = t * t * (3.0 - 2.0 * t);
    // 0.0 => tighter/smaller regions, 1.0 => legacy broad regions.
    2.35 + (1.0 - 2.35) * curved
}

impl World {
    pub(super) fn procedural_block(&self, x: i64, y: i32, z: i64) -> Block {
        let surface_y = self.surface_height(x, z);
        self.procedural_block_with_surface(x, y, z, surface_y)
    }

    pub(super) fn procedural_block_with_surface(
        &self,
        x: i64,
        y: i32,
        z: i64,
        surface_y: i32,
    ) -> Block {
        if y > surface_y {
            if y <= WORLD_SEA_LEVEL {
                return Block::Water;
            }
            return Block::Air;
        }

        if y <= surface_y && should_carve_air(self.seed, x, y, z, surface_y) {
            return Block::Air;
        }

        if y == surface_y && y <= WORLD_SEA_LEVEL {
            return Block::Dirt;
        }
        if y == surface_y && y >= WORLD_OVERWORLD_FLOOR {
            return Block::Grass;
        }
        if y >= surface_y - 2 && y >= WORLD_OVERWORLD_FLOOR - 16 {
            return Block::Dirt;
        }
        crust_block_for_y(y)
    }

    pub(super) fn surface_height(&self, x: i64, z: i64) -> i32 {
        let cfg = self.terrain;
        let fx = x as f32;
        let fz = z as f32;
        let region_multiplier = region_frequency_multiplier(cfg.biome_region_scale);
        let broad_multiplier = region_multiplier.powf(0.55);
        let relief_multiplier = region_multiplier.powf(0.90);

        let (_, macro_frequency_raw) = span_and_frequency(cfg.macro_scale, 0.0015, 0.0800);
        let macro_frequency = macro_frequency_raw * broad_multiplier;
        let (_, detail_frequency_raw) = span_and_frequency(cfg.detail_scale, 0.0050, 0.3000);
        let detail_frequency = detail_frequency_raw * relief_multiplier.powf(0.28);
        let micro_span = cfg.micro_scale.clamp(0.0, 1.0);
        let micro_frequency = span_to_frequency(micro_span, 0.0600, 0.9000);
        let (_, biome_frequency_raw) = span_and_frequency(cfg.biome_scale, 0.0008, 0.0300);
        let biome_frequency = biome_frequency_raw * region_multiplier;
        let (_, valley_frequency_raw) = span_and_frequency(cfg.valley_scale, 0.0015, 0.0800);
        let valley_frequency = valley_frequency_raw * relief_multiplier;
        let (_, dune_frequency_raw) = span_and_frequency(cfg.desert_dune_scale, 0.0050, 0.2000);
        let dune_frequency = dune_frequency_raw * relief_multiplier.powf(0.62);
        let (_, ravine_frequency_raw) = span_and_frequency(cfg.ravine_scale, 0.0010, 0.0800);
        let ravine_frequency = ravine_frequency_raw * relief_multiplier;
        let (_, cliff_frequency_raw) = span_and_frequency(cfg.cliff_scale, 0.0040, 0.1600);
        let cliff_frequency = cliff_frequency_raw * relief_multiplier;

        let biome_warp = fbm_2d(
            self.seed.wrapping_add(0x9E37_79B9),
            fx * biome_frequency * 0.12,
            fz * biome_frequency * 0.12,
            3,
            2.0,
            0.55,
        ) * 84.0;
        let climate_noise = fbm_2d(
            self.seed.wrapping_add(0x7F4A_7C15),
            (fx + biome_warp) * biome_frequency * 0.44,
            (fz - biome_warp * 0.6) * biome_frequency * 0.44,
            4,
            2.0,
            0.5,
        );
        let continent_noise = fbm_2d(
            self.seed.wrapping_add(0x243F_6A88),
            (fx + biome_warp * 0.25) * biome_frequency * 0.24,
            (fz - biome_warp * 0.20) * biome_frequency * 0.24,
            4,
            2.0,
            0.50,
        );
        let biome_blend = cfg.biome_blend.max(0.02);
        let ocean_mask = 1.0
            - smooth_range(
                continent_noise,
                -0.36 - biome_blend * 0.45,
                -0.36 + biome_blend * 0.45,
            );
        let mountain_pref = smooth_range(
            continent_noise,
            0.14 - biome_blend * 0.9,
            0.14 + biome_blend * 0.9,
        );
        let desert_pref = smooth_range(climate_noise, 0.22 - biome_blend, 0.22 + biome_blend)
            * (1.0 - ocean_mask * 0.85);

        let macro_noise = fbm_2d(
            self.seed,
            fx * macro_frequency,
            fz * macro_frequency,
            4,
            2.0,
            0.5,
        );
        let detail_noise = fbm_2d(
            self.seed.wrapping_add(0x6A09_E667),
            fx * detail_frequency,
            fz * detail_frequency,
            3,
            2.0,
            0.55,
        );
        let micro_noise = fbm_2d(
            self.seed.wrapping_add(0xA409_3822),
            fx * micro_frequency,
            fz * micro_frequency,
            2,
            2.2,
            0.45,
        );
        let (mountain_span, mountain_frequency_raw) =
            span_and_frequency(cfg.mountain_scale, 0.0035, 0.0600);
        let mountain_frequency = mountain_frequency_raw * relief_multiplier;
        let mountain_sharpness = cfg.mountain_sharpness.clamp(0.0, 1.0);
        let mountain_base = fbm_2d(
            self.seed.wrapping_add(0xBB67_AE85),
            fx * mountain_frequency * 0.58,
            fz * mountain_frequency * 0.58,
            4,
            2.0,
            0.54,
        )
        .max(0.0)
        .powf(1.18 + mountain_sharpness * 0.38);
        let mountain_ridge = ridge_fbm_2d(
            self.seed.wrapping_add(0xBB67_AE85),
            fx * mountain_frequency,
            fz * mountain_frequency,
            4,
            2.0,
            0.48,
        );
        let w_base = 0.86 - mountain_sharpness * 0.44;
        let w_ridge = 0.52 + mountain_sharpness * 0.74;
        let mountain_noise = (mountain_base * w_base + mountain_ridge * w_ridge)
            / (w_base + w_ridge).max(f32::EPSILON);
        let valley_noise = fbm_2d(
            self.seed.wrapping_add(0x3C6E_F372),
            fx * valley_frequency,
            fz * valley_frequency,
            3,
            2.0,
            0.50,
        )
        .max(0.0)
        .powf(1.35);
        let valley_pref = smooth_range(
            valley_noise,
            0.32 - biome_blend * 0.4,
            0.70 + biome_blend * 0.4,
        ) * (1.0 - mountain_pref * 0.7)
            * (1.0 - ocean_mask * 0.9);
        let forest_pref = smooth_range(
            climate_noise,
            0.12 - biome_blend * 0.7,
            0.48 + biome_blend * 0.9,
        ) * smooth_range(
            continent_noise,
            -0.10 - biome_blend * 0.5,
            0.26 + biome_blend * 0.6,
        ) * (1.0 - ocean_mask * 0.95)
            * (1.0 - desert_pref * 0.8)
            * (1.0 - mountain_pref * 0.65);
        let desert_dune_noise = fbm_2d(
            self.seed.wrapping_add(0x1F83_D9AB),
            fx * dune_frequency,
            fz * dune_frequency,
            4,
            2.15,
            0.52,
        );
        let forest_bump_noise = fbm_2d(
            self.seed.wrapping_add(0xC6EF_3720),
            fx * (detail_frequency * 1.25),
            fz * (detail_frequency * 1.25),
            3,
            2.0,
            0.52,
        );

        let mountain_mask = mountain_pref.max(0.0) * (1.0 - ocean_mask * 0.95);
        let desert_mask = desert_pref.max(0.0) * (1.0 - ocean_mask * 0.9);
        let valley_mask = valley_pref.max(0.0) * (1.0 - forest_pref * 0.7);
        let forest_mask = forest_pref.max(0.0);
        let meadow_mask =
            (1.0 - ocean_mask - mountain_mask - desert_mask - valley_mask - forest_mask)
                .clamp(0.0, 1.0);
        let mask_norm =
            (ocean_mask + mountain_mask + desert_mask + valley_mask + forest_mask + meadow_mask)
                .max(f32::EPSILON);
        let ocean_mask = ocean_mask / mask_norm;
        let mountain_mask = mountain_mask / mask_norm;
        let desert_mask = desert_mask / mask_norm;
        let valley_mask = valley_mask / mask_norm;
        let forest_mask = forest_mask / mask_norm;
        let meadow_mask = meadow_mask / mask_norm;

        let base_height = cfg.base_height
            + macro_noise * cfg.macro_amplitude
            + detail_noise * cfg.detail_amplitude
            + micro_noise * cfg.micro_amplitude;
        let ocean_height = (cfg.base_height - cfg.desert_base_drop - 34.0)
            + macro_noise * (cfg.macro_amplitude * 0.28)
            + detail_noise * (cfg.detail_amplitude * 0.22)
            + micro_noise * (cfg.micro_amplitude * 0.08)
            - valley_noise * (cfg.valley_depth * 0.85);
        let mountain_height = base_height
            + mountain_noise * (cfg.mountain_amplitude * (1.30 + mountain_span * 0.55))
            + micro_noise * (cfg.micro_amplitude * 0.18)
            + cfg.mountain_base_lift
            + cfg.valley_depth * 0.30;
        let meadow_height = base_height
            + mountain_noise * cfg.mountain_amplitude * 0.22
            + micro_noise * (cfg.micro_amplitude * 0.24);
        let valley_height = (cfg.base_height - cfg.valley_depth * 0.62)
            + macro_noise * (cfg.macro_amplitude * 0.55)
            + detail_noise * (cfg.detail_amplitude * 0.55)
            + micro_noise * (cfg.micro_amplitude * 0.20)
            + mountain_noise * (cfg.mountain_amplitude * 0.48);
        let forest_height = (cfg.base_height - cfg.valley_depth * 0.25)
            + macro_noise * (cfg.macro_amplitude * 0.32)
            + detail_noise * (cfg.detail_amplitude * 0.28)
            + micro_noise * (cfg.micro_amplitude * 0.42)
            + forest_bump_noise * (cfg.detail_amplitude * 0.45);
        let desert_height = (cfg.base_height - cfg.desert_base_drop * 0.58)
            + macro_noise * (cfg.macro_amplitude * 0.18)
            + detail_noise * (cfg.detail_amplitude * 0.15)
            + micro_noise * (cfg.micro_amplitude * 0.18)
            + desert_dune_noise * (cfg.desert_dune_amplitude * 0.6);

        let mut height = ocean_height * ocean_mask
            + mountain_height * mountain_mask
            + meadow_height * meadow_mask
            + valley_height * valley_mask
            + forest_height * forest_mask
            + desert_height * desert_mask
            - valley_noise * cfg.valley_depth * (0.72 + 0.28 * meadow_mask);

        let ravine_noise = value_noise_2d(
            self.seed.wrapping_add(0x5BE0_CD19),
            (fx + cfg.ravine_offset_x) * ravine_frequency,
            (fz + cfg.ravine_offset_z) * ravine_frequency,
        );
        let ravine_shape = (1.0 - ravine_noise.abs())
            .max(0.0)
            .powf(cfg.ravine_width.max(1.0));
        let rugged_terrain_mask =
            (mountain_mask * 1.00 + valley_mask * 0.92 + desert_mask * 0.84).clamp(0.0, 1.0);
        let ravine_mask = rugged_terrain_mask * (1.0 - ocean_mask * 0.98);
        height -= ravine_shape * cfg.ravine_strength * ravine_mask;

        // Add larger, surface-reaching chasm cuts so caves have natural ingress points.
        let chasm_noise = value_noise_2d(
            self.seed.wrapping_add(0x8C2D_4F5B),
            (fx + cfg.ravine_offset_x * 0.5) * ravine_frequency * 0.55,
            (fz + cfg.ravine_offset_z * 0.5) * ravine_frequency * 0.55,
        );
        let chasm_shape = (1.0 - chasm_noise.abs())
            .max(0.0)
            .powf((cfg.ravine_width * 0.6).max(1.2));
        let chasm_strength = (cfg.ravine_strength * 3.6).min(40.0);
        let chasm_mask = rugged_terrain_mask.powf(1.15) * (1.0 - ocean_mask * 0.99);
        height -= chasm_shape * chasm_strength * chasm_mask;

        // Carve rivers that separate biome regions across long distances.
        let river_noise = value_noise_2d(
            self.seed.wrapping_add(0xD251_1F53),
            fx * 0.0078,
            fz * 0.0078,
        );
        let river_channel = (1.0 - river_noise.abs()).max(0.0).powf(6.6);
        let river_strength =
            (1.0 - ocean_mask * 0.95) * (1.0 - mountain_mask * 0.45) * (0.55 + forest_mask * 0.35);
        height -= river_channel * (9.0 + cfg.valley_depth * 1.05) * river_strength;

        // Sharpen ocean separations so coastlines are more legible from distance.
        let coast_shelf = smooth_range(ocean_mask, 0.25, 0.88);
        height -= coast_shelf * (2.0 + cfg.valley_depth * 0.12);

        let cliff_biome_mask =
            (mountain_mask * 1.00 + valley_mask * 0.80 + desert_mask * 0.72).clamp(0.0, 1.0);
        if cfg.cliff_strength > 0.0 && cliff_biome_mask > 0.001 {
            let cliff_primary = value_noise_2d(
                self.seed.wrapping_add(0xA54F_F53A),
                fx * cliff_frequency,
                fz * cliff_frequency,
            );
            let cliff_secondary = fbm_2d(
                self.seed.wrapping_add(0x510E_527F),
                fx * cliff_frequency * 0.65,
                fz * cliff_frequency * 0.65,
                3,
                2.0,
                0.52,
            ) * 0.55;
            let cliff_signed = (cliff_primary * 0.75 + cliff_secondary * 0.25).clamp(-1.0, 1.0);
            let edge_distance = cliff_signed.abs();
            let rounding = cfg.cliff_edge_rounding.clamp(0.0, 1.0);
            let edge_width = (0.09 + rounding * 0.24).clamp(0.05, 0.38);
            let edge_core =
                (1.0 - smooth_range(edge_distance, edge_width, edge_width + 0.38)).clamp(0.0, 1.0);
            let local_cliff_strength = cfg.cliff_strength * cliff_biome_mask;

            // Flatten the top lip of cliffs so "hard" cliffs read as broad ledges, not steps.
            let flatness = cfg.cliff_ledge_flatness.clamp(0.0, 1.0);
            if flatness > 0.0 {
                let plateau_target = base_height
                    + mountain_noise * (cfg.mountain_amplitude * 0.58)
                    + macro_noise * (cfg.macro_amplitude * 0.18)
                    + cfg.mountain_base_lift * 0.28;
                let plateau_mask = edge_core.powf(1.15) * flatness * cliff_biome_mask;
                height = height * (1.0 - plateau_mask) + plateau_target * plateau_mask;
            }

            let drop_depth = local_cliff_strength
                * (9.0 + cfg.valley_depth.abs() * 0.45 + cfg.mountain_amplitude.abs() * 0.32);
            let drop_profile = edge_core.powf(0.9 + rounding * 1.25);
            height -= drop_profile * drop_depth;

            // Recessed sidewall pocket to avoid staircase-looking transitions.
            let recess_strength = cfg.cliff_recess_strength.clamp(0.0, 1.0);
            if recess_strength > 0.0 {
                let recess_band = smooth_range(edge_distance, edge_width * 1.5, edge_width * 2.9)
                    * (1.0 - smooth_range(edge_distance, edge_width * 2.9, edge_width * 4.4));
                height -= recess_band * recess_strength * drop_depth * 0.52;
            }

            // Raise and blend the toe of the drop so the cliff base is less jarring.
            let base_smoothing = cfg.cliff_base_smoothing.clamp(0.0, 1.0);
            if base_smoothing > 0.0 {
                let toe_band = smooth_range(edge_distance, edge_width * 2.8, edge_width * 5.0)
                    * (1.0 - smooth_range(edge_distance, edge_width * 5.0, edge_width * 7.5));
                height += toe_band * base_smoothing * drop_depth * 0.34;
            }

            let terrace_step = cfg.terrace_step.abs();
            if terrace_step > 0.001 {
                let terraced = (height / terrace_step).round() * terrace_step;
                let terrace_blend =
                    (local_cliff_strength * (1.0 - rounding * 0.75) * 0.44 * edge_core)
                        .clamp(0.0, 0.6);
                height = height * (1.0 - terrace_blend) + terraced * terrace_blend;
            }
        }

        (height.round() as i32).clamp(WORLD_SURFACE_MIN_Y, WORLD_MAX_Y - 4)
    }

    pub fn surface_height_at(&self, x: i64, z: i64) -> i32 {
        self.surface_height(x, z)
    }

    pub(super) fn palette_at(
        &self,
        block: Block,
        x: i64,
        z: i64,
    ) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let biome = self.biome_at(x, z);
        palette(block, x, z, biome)
    }

    pub(super) fn biome_at(&self, x: i64, z: i64) -> BiomeKind {
        let chunk = Self::world_to_chunk(x, z);
        let primary = self.biome_kind_for_chunk(chunk.0, chunk.1);
        let local_x = x.rem_euclid(CHUNK_SIZE) as f32;
        let local_z = z.rem_euclid(CHUNK_SIZE) as f32;
        let edge_extent = (CHUNK_SIZE - 1) as f32;
        let region_multiplier = region_frequency_multiplier(self.terrain.biome_region_scale);
        let blend_width = (5.5 / region_multiplier.powf(0.45)).clamp(2.2, 5.5);

        let mut scores = [0.05_f32; 6];
        scores[biome_index(primary)] += 1.0;
        let mut any_edge_blend = false;

        let neighbors = [
            (
                (chunk.0 - 1, chunk.1),
                local_x,
                z as f32,
                self.seed.wrapping_add(0xA54F_F53A),
            ),
            (
                (chunk.0 + 1, chunk.1),
                edge_extent - local_x,
                z as f32,
                self.seed.wrapping_add(0x510E_527F),
            ),
            (
                (chunk.0, chunk.1 - 1),
                local_z,
                x as f32,
                self.seed.wrapping_add(0x1F83_D9AB),
            ),
            (
                (chunk.0, chunk.1 + 1),
                edge_extent - local_z,
                x as f32,
                self.seed.wrapping_add(0x5BE0_CD19),
            ),
        ];

        for (neighbor_chunk, edge_distance, along_axis, wave_seed) in neighbors {
            let neighbor = self.biome_kind_for_chunk(neighbor_chunk.0, neighbor_chunk.1);
            if neighbor == primary {
                continue;
            }
            let wave = wave_curve(wave_seed, along_axis, x as f32, z as f32);
            let shifted_distance = edge_distance + wave;
            let t = smooth_range(blend_width - shifted_distance, 0.0, blend_width);
            if t <= 0.001 {
                continue;
            }
            any_edge_blend = true;
            scores[biome_index(neighbor)] += t * 0.95;
            scores[biome_index(primary)] += (1.0 - t) * 0.10;
        }

        if !any_edge_blend {
            return primary;
        }

        let total = scores.iter().sum::<f32>().max(f32::EPSILON);
        let mut threshold = ((hash2_to_unit(self.seed ^ 0x94D0_49BB, x, z) + 1.0) * 0.5) * total;
        for (index, score) in scores.iter().enumerate() {
            threshold -= *score;
            if threshold <= 0.0 {
                return biome_from_index(index);
            }
        }
        primary
    }

    pub(super) fn biome_kind_for_chunk(&self, chunk_x: i64, chunk_z: i64) -> BiomeKind {
        let fx = chunk_x as f32;
        let fz = chunk_z as f32;
        let region_multiplier = region_frequency_multiplier(self.terrain.biome_region_scale);
        let warp_frequency = 0.017 * region_multiplier;
        let region_frequency = 0.041 * region_multiplier;
        let moisture_frequency = 0.048 * region_multiplier;
        let relief_frequency = 0.056 * region_multiplier;
        let warp_amplitude = 18.0 / region_multiplier.powf(0.32);
        let warp_x = value_noise_2d(
            self.seed ^ 0x94D0_49BB_u64 as i64,
            fx * warp_frequency,
            fz * warp_frequency,
        ) * warp_amplitude;
        let warp_z = value_noise_2d(
            self.seed ^ 0x510E_527F_u64 as i64,
            fx * warp_frequency + 37.2,
            fz * warp_frequency - 12.6,
        ) * warp_amplitude;
        let wx = fx + warp_x;
        let wz = fz + warp_z;
        let region = value_noise_2d(
            self.seed ^ 0x6A09E667F3BCC909_u64 as i64,
            wx * region_frequency,
            wz * region_frequency,
        );
        let moisture = value_noise_2d(
            self.seed ^ 0xBB67AE8584CAA73B_u64 as i64,
            wx * moisture_frequency + 21.3,
            wz * moisture_frequency - 37.8,
        );
        let relief = value_noise_2d(
            self.seed ^ 0x3C6EF372FE94F82B_u64 as i64,
            wx * relief_frequency - 17.0,
            wz * relief_frequency + 9.0,
        );
        let continentality = region * 0.72 + relief * 0.28;
        if continentality < -0.34 {
            BiomeKind::Ocean
        } else if relief > 0.34 || continentality > 0.36 {
            BiomeKind::Mountain
        } else if moisture < -0.20 {
            BiomeKind::Desert
        } else if moisture > 0.20 && relief > -0.10 && relief < 0.26 {
            BiomeKind::Forest
        } else if relief > 0.06 || region < -0.04 {
            BiomeKind::Valley
        } else {
            BiomeKind::Meadow
        }
    }
}
