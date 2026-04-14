struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct LightingUniform {
    sun_direction: vec4<f32>,
    moon_direction: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    camera_position: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;
@group(0) @binding(1)
var<uniform> lighting: LightingUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_position = in.position;
    return out;
}

fn terrain_height(world_x: f32, world_z: f32, seed: f32) -> f32 {
    let s = seed * 0.0001;
    let terrain = round(
        sin(world_x * 0.035 + s) * 6.5
            + cos(world_z * 0.028 + s * 2.0) * 5.0
            + sin((world_x + world_z) * 0.012) * 3.0
    );
    return clamp(32.0 + terrain, 2.0, 60.0);
}

fn terrain_shadow(world_pos: vec3<f32>, light_dir: vec3<f32>, seed: f32) -> f32 {
    var shadow = 1.0;
    var distance = 8.0;

    for (var i: i32 = 0; i < 4; i = i + 1) {
        let sample = world_pos + light_dir * distance;
        let terrain_y = terrain_height(sample.x, sample.z, seed);
        if terrain_y > sample.y - 0.15 {
            let weight = 1.0 - f32(i) * 0.18;
            shadow = min(shadow, 1.0 - 0.45 * weight);
        }
        distance = distance + 12.0;
    }

    return shadow;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let moon_dir = normalize(lighting.moon_direction.xyz);
    let sun_intensity = lighting.params.x;
    let moon_intensity = lighting.params.y;
    let ambient = lighting.params.z;
    let seed = lighting.params.w;

    let sun_ndotl = max(dot(n, sun_dir), 0.0);
    let moon_ndotl = max(dot(n, moon_dir), 0.0);
    let soft_sun = smoothstep(0.0, 0.85, sun_ndotl);
    let soft_moon = smoothstep(0.0, 0.95, moon_ndotl);

    var sun_shadow = 1.0;
    if sun_intensity > 0.02 && sun_ndotl > 0.02 {
        sun_shadow = terrain_shadow(in.world_position, sun_dir, seed);
    }
    let sun_light = soft_sun * sun_intensity * sun_shadow;
    let moon_light = soft_moon * moon_intensity * 0.85;

    let surface_y = terrain_height(in.world_position.x, in.world_position.z, seed);
    let depth_below_surface = max(surface_y - in.world_position.y, 0.0);
    let cave_darkness = smoothstep(0.5, 9.0, depth_below_surface);
    let cave_factor = 1.0 - cave_darkness * 0.82;

    let total_light = (ambient + sun_light + moon_light) * cave_factor;
    let base = in.color * total_light;
    return vec4<f32>(base, 1.0);
}
