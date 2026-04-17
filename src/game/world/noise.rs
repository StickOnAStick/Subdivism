pub(super) fn wave_curve(seed: i64, along_axis: f32, x: f32, z: f32) -> f32 {
    let long_wave = (along_axis * 0.115).sin() * 0.95;
    let noise_wave = value_noise_2d(seed, x * 0.16, z * 0.16) * 1.25;
    long_wave + noise_wave
}

pub(super) fn ridge_fbm_2d(
    seed: i64,
    x: f32,
    z: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;

    for octave in 0..octaves {
        let n = value_noise_2d(
            seed.wrapping_add(octave as i64 * 97),
            x * frequency,
            z * frequency,
        );
        let ridge = 1.0 - n.abs();
        sum += ridge * amplitude;
        norm += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    if norm <= f32::EPSILON {
        0.0
    } else {
        (sum / norm) * 2.0 - 1.0
    }
}

pub(super) fn fbm_2d(seed: i64, x: f32, z: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;

    for octave in 0..octaves {
        let n = value_noise_2d(
            seed.wrapping_add(octave as i64 * 131),
            x * frequency,
            z * frequency,
        );
        sum += n * amplitude;
        norm += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    if norm <= f32::EPSILON {
        0.0
    } else {
        sum / norm
    }
}

pub(super) fn value_noise_2d(seed: i64, x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i64;
    let z0 = z.floor() as i64;
    let x1 = x0 + 1;
    let z1 = z0 + 1;
    let tx = smoothstep(x - x.floor());
    let tz = smoothstep(z - z.floor());

    let v00 = hash2_to_unit(seed, x0, z0);
    let v10 = hash2_to_unit(seed, x1, z0);
    let v01 = hash2_to_unit(seed, x0, z1);
    let v11 = hash2_to_unit(seed, x1, z1);

    let a = lerp(v00, v10, tx);
    let b = lerp(v01, v11, tx);
    lerp(a, b, tz)
}

pub(super) fn smooth_range(value: f32, edge0: f32, edge1: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smoothstep(t)
}

pub(super) fn hash2_to_unit(seed: i64, x: i64, z: i64) -> f32 {
    let mut h = seed as u64;
    h ^= (x as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    h ^= (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;

    let u = (h as f64) / (u64::MAX as f64);
    (u as f32) * 2.0 - 1.0
}

pub(super) fn value_noise_3d(seed: i64, x: f32, y: f32, z: f32) -> f32 {
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let z0 = z.floor() as i64;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let z1 = z0 + 1;

    let tx = smoothstep(x - x.floor());
    let ty = smoothstep(y - y.floor());
    let tz = smoothstep(z - z.floor());

    let c000 = hash3_to_unit(seed, x0, y0, z0);
    let c100 = hash3_to_unit(seed, x1, y0, z0);
    let c010 = hash3_to_unit(seed, x0, y1, z0);
    let c110 = hash3_to_unit(seed, x1, y1, z0);
    let c001 = hash3_to_unit(seed, x0, y0, z1);
    let c101 = hash3_to_unit(seed, x1, y0, z1);
    let c011 = hash3_to_unit(seed, x0, y1, z1);
    let c111 = hash3_to_unit(seed, x1, y1, z1);

    let x00 = lerp(c000, c100, tx);
    let x10 = lerp(c010, c110, tx);
    let x01 = lerp(c001, c101, tx);
    let x11 = lerp(c011, c111, tx);
    let y0 = lerp(x00, x10, ty);
    let y1 = lerp(x01, x11, ty);
    lerp(y0, y1, tz)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash3_to_unit(seed: i64, x: i64, y: i64, z: i64) -> f32 {
    let mut h = seed as u64;
    h ^= (x as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    h ^= (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    let u = (h as f64) / (u64::MAX as f64);
    (u as f32) * 2.0 - 1.0
}
