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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let moon_dir = normalize(lighting.moon_direction.xyz);
    let sun_intensity = lighting.params.x;
    let moon_intensity = lighting.params.y;
    let ambient = lighting.params.z;

    let sun_ndotl = max(dot(n, sun_dir), 0.0);
    let moon_ndotl = max(dot(n, moon_dir), 0.0);
    let soft_sun = smoothstep(0.0, 0.85, sun_ndotl);
    let soft_moon = smoothstep(0.0, 0.95, moon_ndotl);

    let sky_visibility = smoothstep(-0.35, 0.90, dot(n, vec3<f32>(0.0, 1.0, 0.0)));
    let ambient_light = ambient * mix(0.70, 1.0, sky_visibility);
    let sun_light = soft_sun * sun_intensity;
    let moon_light = soft_moon * moon_intensity * 0.82;
    let underground = smoothstep(6.0, -6.0, in.world_position.y);
    let depth_dim = mix(1.0, 0.88, underground);

    let total_light = clamp(ambient_light + sun_light + moon_light, 0.06, 2.2) * depth_dim;
    let base = in.color * total_light;
    return vec4<f32>(base, 1.0);
}
