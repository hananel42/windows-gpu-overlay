// -------------------
//  Made using AI.
// -------------------
use real_gpu_app::hooks::{EventResult, MouseButton, OverlayEvent};
use real_gpu_app::{Canvas, OverlayContext, OverlayGPUApp, run};
use std::borrow::Cow;
use std::time::Instant;
use real_gpu_app::canvas::{Simple2DEngine, Text};
use real_gpu_app::screen_capture::DxgiScreenCapture;

// ============================================================
// --- קבועי מערכת לכוונון והתאמה אישית (CONFIG CONSTANTS) ---
// ============================================================
const MAX_WAVES: usize = 10;          // מספר הגלים המקסימלי שיכולים לרוץ בו-זמנית
// ============================================================
// WGSL SHADER SOURCE - תמיכה בריבוי גלים ותיקון יחס מסך
// ============================================================
const SHADER_SRC: &str = r#"
struct Wave {
    click_time: f32,
    _pad: f32,
    click_pos: vec2<f32>,
};

struct Uniforms {
    time: f32,
    wave_count: u32,
    screen_size: vec2<f32>,
    waves: array<Wave, 10>, // חייב להתאים ל-MAX_WAVES ב-Rust
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var screen_texture: texture_2d<f32>;
@group(0) @binding(2) var screen_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) << 2u) - 1.0;
    let y = f32(i32(vertex_index & 2u) << 1u) - 1.0;

    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, -y * 0.5 + 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    var wave_uv = uv;

    // חישוב יחס הגובה-רוחב של המסך למניעת מתיחת הגל לאליפסה
    let aspect = uniforms.screen_size.x / uniforms.screen_size.y;

    // לולאה על כל הגלים הפעילים וצבירת העיוותים שלהם
    for (var i: u32 = 0u; i < uniforms.wave_count; i = i + 1u) {
        let wave = uniforms.waves[i];
        let dt = uniforms.time - wave.click_time;


        let duration = 1.5;
        let speed = 0.7;
        let thickness = 0.08;

        if (dt > 0.0 && dt < duration) {
            // תיקון וקטור הכיוון והמרחק לפי יחס המסך כדי לקבל עיגול מושלם
            let dir = uv - wave.click_pos;
            let corrected_dir = vec2<f32>(dir.x * aspect, dir.y);
            let dist = length(corrected_dir);

            let current_radius = dt * speed;
            let dist_from_wave = abs(dist - current_radius);

            if (dist_from_wave < thickness) {
                // דעיכה חלקה לפי זמן ומרחק מחזית הגל
                let fade = (1.0 - dt / duration) * (1.0 - dist_from_wave / thickness);

                // חישוב עיוות סינוס
                let wave_sin = sin(dist * 60.0 - dt * 30.0);

                // הסטת ה-UV (נרמול מחדש ללא ה-aspect כדי לא לעוות את הטקסטורה עצמה)
                wave_uv += normalize(dir) * wave_sin * 0.02 * fade;
            }
        }
    }

    return textureSample(screen_texture, screen_sampler, wave_uv);
}
"#;

// ============================================================
// RUST STRUCTURES - מבנים מיושרים ומותאמים לזיכרון ה-GPU
// ============================================================
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveData {
    click_time: f32,
    _pad: f32, // יישור (Padding) ל-8 בתים עבור ה-vec2 הבא ב-WGSL
    click_pos: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct RippleUniforms {
    time: f32,
    wave_count: u32,
    screen_size: [f32; 2],
    waves: [WaveData; MAX_WAVES],
}

pub struct ScreenWaveApp {
    capture: Option<DxgiScreenCapture>,
    render_pipeline: Option<wgpu::RenderPipeline>,
    bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,
    start_time: Instant,
    engine: Option<Simple2DEngine>,
    fps_label: Option<Text>,
    waves: Vec<WaveData>,
    next_wave_index: usize,
    screen_size: [f32; 2],
    // משתנים חדשים לניהול וחישוב ה-FPS
    fps_history: Vec<f32>,
    history_index: usize,
    last_ui_update: Instant,
    show_info:bool,
}

impl ScreenWaveApp {
    pub fn new() -> Self {
        Self {
            capture: None,
            render_pipeline: None,
            bind_group: None,
            uniform_buffer: None,
            start_time: Instant::now(),
            engine: None,
            fps_label: None,
            // אתחול מערך הגלים עם ערכים שליליים כדי שלא יופעלו מיד
            waves: vec![WaveData { click_time: -10.0, _pad: 0.0, click_pos: [0.0, 0.0] }; MAX_WAVES],
            next_wave_index: 0,
            screen_size: [1920.0, 1080.0], // ערך ברירת מחדל, יתעדכן דינמית ב-init

            // אתחול המשתנים החדשים (מערך היסטוריה בגודל 60 פריימים לקבלת ממוצע יציב)
            fps_history: vec![0.0; 60],
            history_index: 0,
            last_ui_update: Instant::now(),
            show_info: true,
        }
    }
}

impl OverlayGPUApp for ScreenWaveApp {
    fn init(&mut self, context: &mut OverlayContext) {
        context.hide_from_capture(true);
        let device = context.device();
        self.screen_size = [context.width() as f32, context.height() as f32];

        let capture = DxgiScreenCapture::new(device)
            .expect("Critical: DXGI Capture Init Failed");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Multi-Wave Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER_SRC)),
        });

        // יצירת הבאפר בגודל המדויק של ה-Struct הגדול הכולל את מערך הגלים
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: size_of::<RippleUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(capture.texture_view()) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wave Pipeline"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bind_layout)], immediate_size: 0 })),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.format(),
                    write_mask: wgpu::ColorWrites::ALL,
                    blend: None,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        self.engine = Some(Simple2DEngine::new(context));
        self.fps_label = Some(self.engine.as_mut().unwrap().text("\n | FPS: 0  \n | 0.0ms \n | Active Waves: 0 \n | [i] to toggle", 20.0, 20.0, 24.0, (0, 255, 0, 255)));

        self.capture = Some(capture);
        self.render_pipeline = Some(render_pipeline);
        self.bind_group = Some(bind_group);
        self.uniform_buffer = Some(uniform_buffer);
    }

    fn handler(&mut self, context: &mut OverlayContext, event: OverlayEvent) -> EventResult {
        match event {
            OverlayEvent::KeyDown { vk } => {
                match vk {
                    27 => {
                        context.close().unwrap();
                        EventResult::Consumed
                    }
                    73 => {
                        self.show_info = !self.show_info;
                        EventResult::Consumed
                    }
                    _ => {EventResult::Propagated}
                }

            },
            // שימוש באירוע עכבר שמספק קואורדינטות x ו-y של הלחיצה
            OverlayEvent::MouseDown { button: MouseButton::Left } => {
                let (x,y) = context.mouse_position();
                let current_time = self.start_time.elapsed().as_secs_f32();

                // המרת קואורדינטות מסך פיקסליות לטווח UV של [0.0, 1.0]
                let uv_x = x as f32 / self.screen_size[0];
                let uv_y = y as f32 / self.screen_size[1];

                // עדכון הגל הבא בתור (Ring Buffer)
                self.waves[self.next_wave_index] = WaveData {
                    click_time: current_time,
                    _pad: 0.0,
                    click_pos: [uv_x, uv_y],
                };

                // קידום האינדקס בצורה מחזורית
                self.next_wave_index = (self.next_wave_index + 1) % MAX_WAVES;

                EventResult::Propagated
            }
            _ => EventResult::Propagated,
        }
    }

    fn update(&mut self, context: &mut OverlayContext, delta: f32) {
        let elapsed = self.start_time.elapsed().as_secs_f32();

        // העתקת המערך המקומי לתוך מבנה ה-Uniforms המלא
        let mut fixed_waves = [WaveData { click_time: -10.0, _pad: 0.0, click_pos: [0.0, 0.0] }; MAX_WAVES];
        fixed_waves.copy_from_slice(&self.waves);

        let data = RippleUniforms {
            time: elapsed,
            wave_count: MAX_WAVES as u32,
            screen_size: self.screen_size,
            waves: fixed_waves,
        };

        context.queue().write_buffer(
            self.uniform_buffer.as_ref().unwrap(),
            0,
            bytemuck::bytes_of(&data)
        );

        // --- חישוב FPS חכם ויציב יותר (ממוצע נע) ---
        if delta > 0.0 {
            self.fps_history[self.history_index] = delta;
            self.history_index = (self.history_index + 1) % self.fps_history.len();
        }

        // עדכון הטקסט על המסך בקצב מתון (כל 100 מילישניות) כדי למנוע ריצוד בעיניים
        if self.last_ui_update.elapsed().as_millis() >= 100 {
            let sum_delta: f32 = self.fps_history.iter().sum();
            let avg_delta = sum_delta / self.fps_history.len() as f32;

            let (avg_fps, frame_time_ms) = if avg_delta > 0.0 {
                (1.0 / avg_delta, avg_delta * 1000.0)
            } else {
                (0.0, 0.0)
            };

            // ספירת גלים פעילים כרגע על המסך (לפי משך זמן האנימציה שמוגדר כ-1.5 שניות ב-Shader)
            let active_waves = self.waves.iter()
                .filter(|w| elapsed - w.click_time > 0.0 && elapsed - w.click_time < 1.5)
                .count();

            let info_text = format!(
                "| FPS: {:.0} \n| Frame time:{:.1}ms \n| Active Waves: {}/{} \n|\n|Press [i] to show/hide info.",
                avg_fps, frame_time_ms, active_waves, MAX_WAVES
            );

            self.fps_label.as_mut().unwrap().update(
                self.engine.as_mut().unwrap(),
                info_text.as_str()
            );

            self.last_ui_update = Instant::now();
        }
    }

    fn render(&mut self, canvas: Canvas) {
        if let Some(ref mut cap) = self.capture {
            cap.update_and_get_view(canvas.queue);
        }

        let mut encoder = canvas.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Wave Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &canvas.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }),
                        store: wgpu::StoreOp::Store
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if let (Some(pipeline), Some(bind_group)) = (&self.render_pipeline, &self.bind_group) {
                rpass.set_pipeline(pipeline);
                rpass.set_bind_group(0, bind_group, &[]);
                rpass.draw(0..3, 0..1);
            }
        }
        canvas.queue.submit(std::iter::once(encoder.finish()));

        if self.show_info {
            let mut drawer = self.engine.as_mut().unwrap().drawer(&canvas);
            drawer.draw_text(self.fps_label.as_ref().unwrap());
            drawer.draw_rect(0,0,300,130,(155,155,155,50).into())
        }
    }
}

fn main() {
    let mut app = ScreenWaveApp::new();
    if let Err(err) = run(&mut app) {
        eprintln!("Error running app: {}", err);
    }
}