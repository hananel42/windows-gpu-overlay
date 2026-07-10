struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct ScreenSize {
    width: f32,
    height: f32,
};

@group(0) @binding(0) var<uniform> screen: ScreenSize;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let nx = (model.position.x / screen.width) * 2.0 - 1.0;
    let ny = 1.0 - (model.position.y / screen.height) * 2.0;
    out.clip_position = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv = model.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.uv);
}