// ה-Struct שמחזיק את גודל המסך בפיקסלים
struct Uniforms {
    width: f32,
    height: f32,
    _pad: vec2<f32>,
};

// אומרים לשיידר לקרוא את הבאפר מבינד גרופ 0, חריץ 0
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;


struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,         // <--- חדש
    @location(3) shape_type: f32,      // <--- חדש
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,         // <--- חדש
    @location(2) shape_type: f32,      // <--- חדש
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let nx = (model.position.x / uniforms.width) * 2.0 - 1.0;
    let ny = 1.0 - (model.position.y / uniforms.height) * 2.0;

    out.clip_position = vec4<f32>(nx, ny, 0.0, 1.0);

    out.color = model.color;
    out.uv = model.uv;
    out.shape_type = model.shape_type;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.shape_type < 0.5) {
        return in.color;
    }

    let distance = length(in.uv);

    let edge_blur = fwidth(distance);
    // smoothstep מייצר מעבר חלק ב-Alpha בין תוך העיגול למחוץ לו
    let alpha = 1.0 - smoothstep(1.0 - edge_blur, 1.0, distance);

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}