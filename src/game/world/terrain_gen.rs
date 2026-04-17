use super::*;
use super::noise::{
    fbm_2d, hash2_to_unit, ridge_fbm_2d, smooth_range, value_noise_2d, wave_curve,
};
use super::palette::palette;

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
            return Block::Air;
        }

        if y <= surface_y && should_carve_cave(self.seed, x, y, z, surface_y) {
            return Block::Air;
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

        let biome_warp = fbm_2d(
            self.seed.wrapping_add(0x9E37_79B9),
            fx * cfg.biome_scale * 0.12,
            fz * cfg.biome_scale * 0.12,
            3,
            2.0,
            0.55,
        ) * 84.0;
        let climate_noise = fbm_2d(
            self.seed.wrapping_add(0x7F4A_7C15),
            (fx + biome_warp) * cfg.biome_scale * 0.44,
            (fz - biome_warp * 0.6) * cfg.biome_scale * 0.44,
            4,
            2.0,
            0.5,
        );
        let continent_noise = fbm_2d(
            self.seed.wrapping_add(0x243F_6A88),
            (fx + biome_warp * 0.25) * cfg.biome_scale * 0.24,
            (fz - biome_warp * 0.20) * cfg.biome_scale * 0.24,
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

        let macro_noise = fbm_2d(self.seed, fx * cfg.macro_scale, fz * cfg.macro_scale, 4, 2.0, 0.5);
        let detail_noise = fbm_2d(
            self.seed.wrapping_add(0x6A09_E667),
            fx * cfg.detail_scale,
            fz * cfg.detail_scale,
            3,
            2.0,
            0.55,
        );
        let mountain_noise = ridge_fbm_2d(
            self.seed.wrapping_add(0xBB67_AE85),
            fx * cfg.mountain_scale,
            fz * cfg.mountain_scale,
            4,
            2.0,
            0.48,
        );
        let valley_noise = fbm_2d(
            self.seed.wrapping_add(0x3C6E_F372),
            fx * cfg.valley_scale,
            fz * cfg.valley_scale,
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
        let desert_dune_noise = fbm_2d(
            self.seed.wrapping_add(0x1F83_D9AB),
            fx * cfg.desert_dune_scale,
            fz * cfg.desert_dune_scale,
            4,
            2.15,
            0.52,
        );

        let mountain_mask = mountain_pref.max(0.0) * (1.0 - ocean_mask * 0.95);
        let desert_mask = desert_pref.max(0.0) * (1.0 - ocean_mask * 0.9);
        let valley_mask = valley_pref.max(0.0);
        let meadow_mask =
            (1.0 - ocean_mask - mountain_mask - desert_mask - valley_mask).clamp(0.0, 1.0);
        let mask_norm = (ocean_mask + mountain_mask + desert_mask + valley_mask + meadow_mask)
            .max(f32::EPSILON);
        let ocean_mask = ocean_mask / mask_norm;
        let mountain_mask = mountain_mask / mask_norm;
        let desert_mask = desert_mask / mask_norm;
        let valley_mask = valley_mask / mask_norm;
        let meadow_mask = meadow_mask / mask_norm;

        let base_height = cfg.base_height
            + macro_noise * cfg.macro_amplitude
            + detail_noise * cfg.detail_amplitude;
        let ocean_height = (cfg.base_height - cfg.desert_base_drop - 8.0)
            + macro_noise * (cfg.macro_amplitude * 0.28)
            + detail_noise * (cfg.detail_amplitude * 0.22)
            - valley_noise * (cfg.valley_depth * 0.45);
        let mountain_height =
            base_height
                + mountain_noise * (cfg.mountain_amplitude * 1.65)
                + cfg.mountain_base_lift
                + cfg.valley_depth * 0.35;
        let meadow_height = base_height + mountain_noise * cfg.mountain_amplitude * 0.35;
        let valley_height = (cfg.base_height - cfg.valley_depth * 1.35)
            + macro_noise * (cfg.macro_amplitude * 0.55)
            + detail_noise * (cfg.detail_amplitude * 0.55)
            + mountain_noise * (cfg.mountain_amplitude * 0.18);
        let desert_height = (cfg.base_height - cfg.desert_base_drop * 0.35)
            + macro_noise * (cfg.macro_amplitude * 0.42)
            + detail_noise * (cfg.detail_amplitude * 0.30)
            + desert_dune_noise * cfg.desert_dune_amplitude;

        let mut height = ocean_height * ocean_mask
            + mountain_height * mountain_mask
            + meadow_height * meadow_mask
            + valley_height * valley_mask
            + desert_height * desert_mask
            - valley_noise * cfg.valley_depth * (0.72 + 0.28 * meadow_mask);

        let ravine_noise = value_noise_2d(
            self.seed.wrapping_add(0x5BE0_CD19),
            (fx + cfg.ravine_offset_x) * cfg.ravine_scale,
            (fz + cfg.ravine_offset_z) * cfg.ravine_scale,
        );
        let ravine_shape = (1.0 - ravine_noise.abs())
            .max(0.0)
            .powf(cfg.ravine_width.max(1.0));
        let ravine_mask = (0.45 + 0.55 * mountain_mask.max(desert_mask).max(valley_mask))
            * (1.0 - ocean_mask * 0.95);
        height -= ravine_shape * cfg.ravine_strength * ravine_mask;

        // Add larger, surface-reaching chasm cuts so caves have natural ingress points.
        let chasm_noise = value_noise_2d(
            self.seed.wrapping_add(0x8C2D_4F5B),
            (fx + cfg.ravine_offset_x * 0.5) * cfg.ravine_scale * 0.55,
            (fz + cfg.ravine_offset_z * 0.5) * cfg.ravine_scale * 0.55,
        );
        let chasm_shape = (1.0 - chasm_noise.abs())
            .max(0.0)
            .powf((cfg.ravine_width * 0.6).max(1.2));
        let chasm_strength = (cfg.ravine_strength * 3.6).min(40.0);
        let chasm_mask =
            (0.25 + 0.75 * mountain_mask.max(valley_mask).max(desert_mask)) * (1.0 - ocean_mask * 0.98);
        height -= chasm_shape * chasm_strength * chasm_mask;

        if cfg.cliff_strength > 0.0 {
            let cliff_mask = fbm_2d(
                self.seed.wrapping_add(0xA54F_F53A),
                fx * cfg.cliff_scale,
                fz * cfg.cliff_scale,
                2,
                2.0,
                0.5,
            )
            .abs()
            .powf(1.4);
            let terraced = (height / cfg.terrace_step).round() * cfg.terrace_step;
            let blend = (cliff_mask * cfg.cliff_strength).clamp(0.0, 1.0);
            height = height * (1.0 - blend) + terraced * blend;
        }

        (height.round() as i32).clamp(WORLD_OVERWORLD_FLOOR, WORLD_MAX_Y - 4)
    }

    pub fn surface_height_at(&self, x: i64, z: i64) -> i32 {
        self.surface_height(x, z)
    }

    pub(super) fn palette_at(&self, block: Block, x: i64, z: i64) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let biome = self.biome_at(x, z);
        palette(block, x, z, biome)
    }

    pub(super) fn biome_at(&self, x: i64, z: i64) -> BiomeKind {
        let chunk = Self::world_to_chunk(x, z);
        let primary = self.biome_kind_for_chunk(chunk.0, chunk.1);
        let local_x = x.rem_euclid(CHUNK_SIZE) as f32;
        let local_z = z.rem_euclid(CHUNK_SIZE) as f32;
        let edge_extent = (CHUNK_SIZE - 1) as f32;
        let blend_width = 5.5_f32;

        let mut scores = [0.05_f32; 5];
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
        let warp_x = value_noise_2d(
            self.seed ^ 0x94D0_49BB_u64 as i64,
            fx * 0.010,
            fz * 0.010,
        ) * 18.0;
        let warp_z = value_noise_2d(
            self.seed ^ 0x510E_527F_u64 as i64,
            fx * 0.010 + 37.2,
            fz * 0.010 - 12.6,
        ) * 18.0;
        let wx = fx + warp_x;
        let wz = fz + warp_z;
        let region = value_noise_2d(
            self.seed ^ 0x6A09E667F3BCC909_u64 as i64,
            wx * 0.022,
            wz * 0.022,
        );
        let moisture = value_noise_2d(
            self.seed ^ 0xBB67AE8584CAA73B_u64 as i64,
            wx * 0.027 + 21.3,
            wz * 0.027 - 37.8,
        );
        let relief = value_noise_2d(
            self.seed ^ 0x3C6EF372FE94F82B_u64 as i64,
            wx * 0.032 - 17.0,
            wz * 0.032 + 9.0,
        );
        let continentality = region * 0.72 + relief * 0.28;
        if continentality < -0.34 {
            BiomeKind::Ocean
        } else if relief > 0.30 || continentality > 0.34 {
            BiomeKind::Mountain
        } else if moisture < -0.16 {
            BiomeKind::Desert
        } else if region < -0.02 && moisture > 0.03 {
            BiomeKind::Valley
        } else {
            BiomeKind::Meadow
        }
    }
}
