// NacreEngine PBR shader — metallic-roughness workflow, Cook-Torrance BRDF,
// direct lighting only (directional + point). Outputs LINEAR color; the target
// is expected to be an sRGB-encoded view that performs the final encoding.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
};

struct Model {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct Light {
    pos_or_dir: vec4<f32>,
    color: vec4<f32>,
    kind: u32,
    intensity: f32,
    range: f32,
    _pad: f32,
};

struct Lights {
    lights: array<Light, 8>,
    count: u32,
};

struct Material {
    base_color: vec4<f32>,
    metallic: f32,
    roughness: f32,
    flags: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> model: Model;
@group(0) @binding(2) var<uniform> lights: Lights;

@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var normal_tex: texture_2d<f32>;
@group(1) @binding(2) var ao_tex: texture_2d<f32>;
@group(1) @binding(3) var tex_sampler: sampler;

const PI: f32 = 3.14159265359;
const LIGHT_DIRECTIONAL: u32 = 0u;
const FLAG_HAS_NORMAL_MAP: u32 = 1u;
const FLAG_HAS_OCCLUSION_MAP: u32 = 2u;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec3<f32>,
    @location(3) tangent_w: f32,
    @location(4) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let world = model.model * vec4<f32>(in.position, 1.0);
    out.world_pos = world.xyz;
    out.clip_pos = camera.view_proj * world;
    let nm = mat3x3<f32>(
        model.normal_matrix[0].xyz,
        model.normal_matrix[1].xyz,
        model.normal_matrix[2].xyz,
    );
    out.world_normal = nm * in.normal;
    out.world_tangent = nm * in.tangent.xyz;
    out.tangent_w = in.tangent.w;
    out.uv = in.uv;
    return out;
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-7);
}

fn geometry_schlick_ggx(n_dot_x: f32, k: f32) -> f32 {
    return n_dot_x / (n_dot_x * (1.0 - k) + k);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return geometry_schlick_ggx(n_dot_v, k) * geometry_schlick_ggx(n_dot_l, k);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Sample maps unconditionally (uniform control flow), then select by flag.
    let geo_n = normalize(in.world_normal);
    let t = normalize(in.world_tangent - geo_n * dot(geo_n, in.world_tangent));
    let b = cross(geo_n, t) * in.tangent_w;
    let tbn = mat3x3<f32>(t, b, geo_n);
    let sampled = textureSample(normal_tex, tex_sampler, in.uv).xyz * 2.0 - 1.0;
    let mapped_n = normalize(tbn * sampled);
    let ao_sampled = textureSample(ao_tex, tex_sampler, in.uv).r;

    var n = geo_n;
    if ((material.flags & FLAG_HAS_NORMAL_MAP) != 0u) {
        n = mapped_n;
    }
    var ao = 1.0;
    if ((material.flags & FLAG_HAS_OCCLUSION_MAP) != 0u) {
        ao = ao_sampled;
    }

    let base_color = material.base_color.rgb;
    let metallic = material.metallic;
    let roughness = max(material.roughness, 0.045);

    let v = normalize(camera.camera_pos.xyz - in.world_pos);
    let n_dot_v = max(dot(n, v), 1e-4);
    let f0 = mix(vec3<f32>(0.04), base_color, metallic);

    var lo = vec3<f32>(0.0);
    let count = min(lights.count, 8u);
    for (var i = 0u; i < count; i = i + 1u) {
        let light = lights.lights[i];

        var l: vec3<f32>;
        var radiance: vec3<f32>;
        if (light.kind == LIGHT_DIRECTIONAL) {
            l = normalize(-light.pos_or_dir.xyz);
            radiance = light.color.rgb * light.intensity;
        } else {
            let to_light = light.pos_or_dir.xyz - in.world_pos;
            let dist = length(to_light);
            l = to_light / max(dist, 1e-4);
            let atten = 1.0 / max(dist * dist, 1e-4);
            var range_factor = 1.0;
            if (light.range > 0.0) {
                let f = clamp(1.0 - pow(dist / light.range, 4.0), 0.0, 1.0);
                range_factor = f * f;
            }
            radiance = light.color.rgb * light.intensity * atten * range_factor;
        }

        let h = normalize(v + l);
        let n_dot_l = max(dot(n, l), 0.0);
        let n_dot_h = max(dot(n, h), 0.0);
        let h_dot_v = max(dot(h, v), 0.0);

        let ndf = distribution_ggx(n_dot_h, roughness);
        let g = geometry_smith(n_dot_v, n_dot_l, roughness);
        let f = fresnel_schlick(h_dot_v, f0);

        let specular = (ndf * g * f) / (4.0 * n_dot_v * n_dot_l + 1e-4);
        let ks = f;
        let kd = (vec3<f32>(1.0) - ks) * (1.0 - metallic);

        lo = lo + (kd * base_color / PI + specular) * radiance * n_dot_l;
    }

    let color = lo * ao;
    return vec4<f32>(color, material.base_color.a);
}
