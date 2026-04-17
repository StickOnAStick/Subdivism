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
    style: vec4<f32>,
    fog: vec4<f32>,
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sun_intensity = lighting.params.x;
    let moon_intensity = lighting.params.y;
    let ambient = lighting.params.z;
    let vibrance = clamp(lighting.style.w, 0.5, 1.8);
    let shadow_softness = clamp(lighting.style.x, 0.0, 1.5);
    let shadow_contrast = clamp(lighting.style.y, 0.0, 2.5);
    let far_shadow_lift = clamp(lighting.style.z, 0.0, 0.8);
    let fog_strength = clamp(lighting.fog.x, 0.0, 1.2);
    let fog_start = max(lighting.fog.y, 1.0);
    let fog_end = max(lighting.fog.z, fog_start + 1.0);
    let atmosphere_strength = clamp(lighting.fog.w, 0.0, 1.0);

    let n = normalize(in.normal);
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let moon_dir = normalize(lighting.moon_direction.xyz);
    let sun_diffuse_raw = max(dot(n, sun_dir), 0.0);
    let moon_diffuse_raw = max(dot(n, moon_dir), 0.0);
    let soften_power = mix(1.8, 0.55, clamp(shadow_softness / 1.5, 0.0, 1.0));
    let sun_diffuse = pow(sun_diffuse_raw, max(soften_power, 0.05));
    let moon_diffuse = pow(moon_diffuse_raw, max(soften_power, 0.05));
    let face_lift = mix(0.82, 1.03, max(n.y, 0.0));
    let global_light = clamp(
        ambient
            + sun_intensity * (0.24 + sun_diffuse * (0.76 + 0.32 * shadow_contrast))
            + moon_intensity * (0.12 + moon_diffuse * (0.46 + 0.20 * shadow_contrast)),
        0.16,
        1.28
    );
    let lit_color = in.color * global_light * face_lift;

    let delta_xz = in.world_position.xz - lighting.camera_position.xz;
    let horizontal_distance = length(delta_xz);
    let fog_t = smoothstep(fog_start, fog_end, horizontal_distance) * fog_strength;

    let sky_horizon = mix(lighting.sky_bottom.rgb, lighting.sky_top.rgb, 0.55);
    let atmosphere_color = mix(sky_horizon, vec3<f32>(0.72, 0.82, 0.96), atmosphere_strength);
    let fog_mix = clamp(fog_t, 0.0, 1.0);
    let shadow_lift = mix(1.0, 1.0 + far_shadow_lift, fog_mix);
    let fogged = mix(lit_color * shadow_lift, atmosphere_color, fog_mix);
    let luma = dot(fogged, vec3<f32>(0.299, 0.587, 0.114));
    let base = mix(vec3<f32>(luma), fogged, vibrance);
    return vec4<f32>(base, 1.0);
}
