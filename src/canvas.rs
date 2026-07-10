use crate::OverlayContext;
use glyphon::Shaping;
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureView};


const MAX_VERTICES:usize = 80_000;
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],    // <--- חדש! קואורדינטות פנימיות
    pub shape_type: f32, // <--- חדש! 0.0 = משולש, 1.0 = עיגול
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    width: f32,
    height: f32,
    _padding: [f32; 2], // מבטיח שהגודל הכולל בזיכרון יהיה 16 בייטים
}

pub struct Canvas<'a> {
    pub device: &'a Device,
    pub queue: &'a Queue,
    pub view: TextureView,
    pub width: i32,
    pub height: i32,
}

pub struct Simple2DEngine {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    image_pipeline: wgpu::RenderPipeline,
    image_bind_group_layout: wgpu::BindGroupLayout,
    image_vertex_buffer: wgpu::Buffer,

    pub font_system: glyphon::FontSystem,
    pub swash_cache: glyphon::SwashCache,
    pub text_cache: glyphon::Cache,       // <--- חדש ב-API
    pub text_viewport: glyphon::Viewport, // <--- חדש ב-API
    pub text_atlas: glyphon::TextAtlas,
    pub text_renderer: glyphon::TextRenderer,
}
impl Simple2DEngine {
    pub fn new(context: &mut OverlayContext) -> Self {
        let format = context.format();
        let device = context.device();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Canvas Triangle Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/triangle.wgsl").into()),
        });

        // 1. הגדרת ה"פורמט" של הבינד גרופ
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX, // רק ה-Vertex Shader צריך לדעת את גודל המסך
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let initial_size = Uniforms {
            width: context.width() as f32,
            height: context.height() as f32,
            _padding: [0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Size Uniform Buffer"),
            contents: bytemuck::cast_slice(&[initial_size]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 3. חיבור הבאפר ללייאאוט (יצירת הבינד גרופ האמיתי)
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Canvas Pipeline Layout"),
            bind_group_layouts: &[Some(&uniform_bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Canvas Render Pipeline"),
            layout: Some(&layout),

            // 1. הגדרות שלב ה-Vertex
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"), // שם הפונקציה בשיידר
                compilation_options: Default::default(),
                // במקום כל המערך הידני, wgpu מציעה קיצור דרך מובנה:
                buffers: &[Option::from(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // position
                        1 => Float32x4, // color
                        2 => Float32x2, // uv
                        3 => Float32,   // shape_type
                    ],
                })],
            },

            // 2. הגדרות שלב ה-Fragment (צביעת הפיקסלים)
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // הצינור חייב לדעת מראש מה פורמט ה-Texture שהוא הולך לצייר עליו!
                    // בדרך כלל זה ה-format שמקבלים מה-surface.get_capabilities
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING), // איך לשלב צבעים (כאן: דריסה פשוטה)
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),

            // 3. איך לחבר את הקודקודים לצורות
            primitive: wgpu::PrimitiveState {
                // TriangleList אומר שכל 3 קודקודים רציפים בבאפר הופכים למשולש עצמאי
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // כיוון השעון של הקודקודים
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },

            depth_stencil: None, // אין לנו תלת-ממד כרגע, אז אין צורך בבדיקת עומק
            multisample: wgpu::MultisampleState::default(),

            cache: None,
            multiview_mask: None,
        });


        let buffer_size = (MAX_VERTICES * std::mem::size_of::<Vertex>()) as wgpu::BufferAddress;

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Canvas Vertex Buffer"),
            size: buffer_size,
            // COPY_DST אומר שאנחנו יכולים להעתיק לבאפר הזה נתונים מה-CPU
            // VERTEX אומר שהבאפר הזה משמש כקלט של קודקודים ל-Pipeline
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 1. טעינת השיידר הייעודי לתמונות (image.wgsl)
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Canvas Image Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
        });

        // 2. הגדרת הלייאאוט של הטקסטורה הבודדת (Group 1) עבור תמונות
        let image_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Image Texture Bind Group Layout"),
                entries: &[
                    // הטקסטורה (binding 0)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2, // טקסטורה 2D רגילה!
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // הסאמפלר (binding 1)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // 3. יצירת ה-Pipeline Layout של התמונות (משתמש באותו קבוצה 0 של ה-Uniform)
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Canvas Image Pipeline Layout"),
                bind_group_layouts: &[
                    Option::from(&uniform_bind_group_layout), // group(0) - גודל מסך
                    Option::from(&image_bind_group_layout),   // group(1) - הטקסטורה הדינמית
                ],
                immediate_size: 0,
            });

        // 4. יצירת ה-Render Pipeline של התמונות
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Canvas Image Render Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                // מבנה קודקוד פשוט של תמונה: רק מיקום (vec2) ו-UV (vec2)
                buffers: &[Option::from(wgpu::VertexBufferLayout {
                    array_stride: (std::mem::size_of::<f32>() * 4) as wgpu::BufferAddress, // 4 פלוטים לקודקוד
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // position
                        1 => Float32x2, // uv
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING), // תמיכה בשקיפות של PNG
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        // 5. באפר קודקודים קטן וייעודי לתמונות (מקום ל-6 קודקודים שמייצרים מלבן תמונה)
        let image_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Canvas Image Vertex Buffer"),
            size: (6 * std::mem::size_of::<f32>() * 4) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // בתוך Simple2DEngine::new:
        let mut font_system = glyphon::FontSystem::new_with_fonts(std::iter::empty());

        // 2. טעינה ידנית ומהירה של פונט ספציפי מה-Binary שלך
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../assets/Arial.ttf").to_vec());
        let swash_cache = glyphon::SwashCache::new();

        // 1. יוצרים Cache פנימי של glyphon
        let text_cache = glyphon::Cache::new(device);

        // 2. יוצרים Viewport שמייצג את גודל המסך (ברזולוציה הנוכחית)
        let mut text_viewport = glyphon::Viewport::new(device, &text_cache);
        text_viewport.update(
            &context.queue,
            glyphon::Resolution {
                width: context.width() as u32,
                height: context.height() as u32,
            },
        );

        // 3. יוצרים את ה-TextAtlas
        let mut text_atlas = glyphon::TextAtlas::new(device, &context.queue(), &text_cache, format);

        // 4. יוצרים את ה-TextRenderer ומעבירים לו רפרנס ל-Cache ול-Viewport
        let text_renderer = glyphon::TextRenderer::new(
            &mut text_atlas,
            context.device(),
            wgpu::MultisampleState::default(),
            None,
        );

        Simple2DEngine {
            pipeline,
            layout,
            shader,
            buffer,
            uniform_buffer,
            uniform_bind_group,
            image_pipeline,
            image_bind_group_layout,
            image_vertex_buffer,
            font_system,
            swash_cache,
            text_cache,
            text_viewport,
            text_atlas,
            text_renderer,
        }
    }

    pub fn drawer<'a>(&mut self, canvas: &'a Canvas<'a>) -> Drawer<'a, '_> {
        Drawer {
            engine: self,
            vertexes: Vec::with_capacity(300),
            canvas,
            text_areas: Vec::new(),
        }
    }

    pub fn image_from_rgba(
        &self,
        context: &mut OverlayContext,
        width: u32,
        height: u32,
        rgba_bytes: &[u8],
    ) -> GPUImage {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = context.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("User Image Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // פורמט RGBA סטנדרטי עם שקיפות
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // 2. העתקת מערך הבייטים מה-CPU ישירות לתוך הטקסטורה ב-GPU
        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4), // 4 בייטים לכל פיקסל (R, G, B, A)
                rows_per_image: Some(height),
            },
            size,
        );

        // 3. יצירת ה-View (הדרך של השיידר להסתכל על הטקסטורה)
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 4. יצירת סאמפלר מקומי (קובע איך הפיקסלים יימתחו/יצטמצמו)
        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // 5. חיבור ה-View והסאמפלר ל-Bind Group האמיתי לפי הלייאאוט של ה-Pipeline
        let bind_group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("User Image Bind Group"),
                layout: &self.image_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

        GPUImage {
            width,
            height,
            bind_group,
        }
    }

    pub fn text(
        &mut self,
        content: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: (u8, u8, u8, u8), // מקבל מערך של RGBA כבייטים (0-255) עבור glyphon::Color
    ) -> Text {
        let metrics = glyphon::Metrics::new(font_size, font_size * 1.2);
        let mut buffer = glyphon::Buffer::new(&mut self.font_system, metrics);

        let attrs = glyphon::Attrs::new().family(glyphon::Family::SansSerif);

        // ריצה חד פעמית של ה-Shaping באתחול!
        buffer.set_text(content, &attrs, glyphon::Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        let (r, g, b, a) = color;
        let color = glyphon::Color::rgba(r, g, b, a);

        Text {
            buffer,
            x,
            y,
            color,
        }
    }
}

pub struct GPUImage {
    pub width: u32,
    pub height: u32,
    pub bind_group: wgpu::BindGroup,
}

pub struct Text {
    pub buffer: glyphon::Buffer,
    pub x: f32,
    pub y: f32,
    pub color: glyphon::Color,
}

impl Text {
    pub fn with_box(mut self, max_width: f32, max_height: f32) -> Self {
        self.buffer.set_size(Some(max_width), Some(max_height));
        self
    }

    /// פונקציית עזר למקרה שהטקסט משתנה בזמן ריצה (למשל שינוי ניקוד או FPS)
    pub fn update(&mut self, engine: &mut Simple2DEngine, new_content: &str) {
        let attrs = glyphon::Attrs::new().family(glyphon::Family::SansSerif);
        self.buffer
            .set_text(new_content, &attrs, glyphon::Shaping::Advanced, None);
        self.buffer
            .shape_until_scroll(&mut engine.font_system, false);
    }
}

pub struct Drawer<'a, 'b> {
    engine: &'b mut Simple2DEngine,
    vertexes: Vec<Vertex>,
    canvas: &'a Canvas<'a>,
    text_areas: Vec<glyphon::TextArea<'b>>,
}

pub struct Color {
    rgba: [f32; 4],
}
impl Color {
    pub fn new(rgba: [f32; 4]) -> Self {
        Color { rgba }
    }
    pub fn r(&self) -> f32 {
        self.rgba[0]
    }
    pub fn g(&self) -> f32 {
        self.rgba[1]
    }
    pub fn b(&self) -> f32 {
        self.rgba[2]
    }
    pub fn a(&self) -> f32 {
        self.rgba[3]
    }
}

impl From<[f32; 4]> for Color {
    fn from(rgba: [f32; 4]) -> Self {
        Color { rgba }
    }
}
impl From<(f32, f32, f32, f32)> for Color {
    fn from(rgba: (f32, f32, f32, f32)) -> Self {
        let (r, g, b, a) = rgba;
        Color {
            rgba: [r / 255.0, g / 255.0, b / 255.0, a / 255.0],
        }
    }
}

impl From<(u8, u8, u8, u8)> for Color {
    fn from(rgba: (u8, u8, u8, u8)) -> Self {
        let (r, g, b, a) = rgba;
        Color {
            rgba: [
                (r as f32) / 255.0,
                (g as f32) / 255.0,
                (b as f32) / 255.0,
                (a as f32) / 255.0,
            ],
        }
    }
}
impl Into<(u8, u8, u8, u8)> for Color {
    fn into(self) -> (u8, u8, u8, u8) {
        (
            (self.r() * 255.0) as u8,
            (self.g() * 255.0) as u8,
            (self.b() * 255.0) as u8,
            (self.a() * 255.0) as u8,
        )
    }
}

impl Into<wgpu::Color> for Color {
    fn into(self) -> wgpu::Color {
        let [r, g, b, a] = self.rgba;
        wgpu::Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: a as f64,
        }
    }
}

impl<'a, 'b> Drawer<'a, 'b> {
    pub fn draw_triangle(&mut self, p1: [i32; 2], p2: [i32; 2], p3: [i32; 2], color: Color) {
        self.ensure_capacity(3);
        self.vertexes.push(Vertex {
            position: [p1[0] as f32, p1[1] as f32],
            color: color.rgba,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
        self.vertexes.push(Vertex {
            position: [p2[0] as f32, p2[1] as f32],
            color: color.rgba,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
        self.vertexes.push(Vertex {
            position: [p3[0] as f32, p3[1] as f32],
            color: color.rgba,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
    }
    pub fn draw_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Color) {
        self.ensure_capacity(6);
        let color = color.rgba;

        // חישוב המיקומים בפיקסלים כנקודה צפה
        let x = x as f32;
        let y = y as f32;
        let w = w as f32;
        let h = h as f32;

        let top_left = [x, y];
        let top_right = [x + w, y];

        // במערכת צירים שבה (0,0) זה שמאל למעלה,
        // כדי לרדת למטה במסך אנחנו *מוסיפים* לגובה (y + h).
        let bottom_left = [x, y + h];
        let bottom_right = [x + w, y + h];

        // משולש ראשון (שמאל-למעלה, ימין-למעלה, שמאל-למטה)
        self.vertexes.push(Vertex {
            position: top_left,
            color,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
        self.vertexes.push(Vertex {
            position: top_right,
            color,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
        self.vertexes.push(Vertex {
            position: bottom_left,
            color,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });

        // משולש שני (ימין-למעלה, ימין-למטה, שמאל-למטה)
        self.vertexes.push(Vertex {
            position: top_right,
            color,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
        self.vertexes.push(Vertex {
            position: bottom_right,
            color,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
        self.vertexes.push(Vertex {
            position: bottom_left,
            color,
            uv: [0.0, 0.0],
            shape_type: 0.0,
        });
    }
    pub fn draw_circle(&mut self, cx: i32, cy: i32, r: u32, color: Color) {
        self.ensure_capacity(6);
        let c = color.rgba;
        let shape_type = 1.0;

        let cx = cx as f32;
        let cy = cy as f32;
        let r = r as f32;

        // 4 הפינות של הריבוע החוסם בפיקסלים
        let top_left = [cx - r, cy - r];
        let top_right = [cx + r, cy - r];
        let bottom_left = [cx - r, cy + r];
        let bottom_right = [cx + r, cy + r];

        // משולש 1
        self.vertexes.push(Vertex {
            position: top_left,
            color: c,
            uv: [-1.0, -1.0],
            shape_type,
        });
        self.vertexes.push(Vertex {
            position: top_right,
            color: c,
            uv: [1.0, -1.0],
            shape_type,
        });
        self.vertexes.push(Vertex {
            position: bottom_left,
            color: c,
            uv: [-1.0, 1.0],
            shape_type,
        });

        // משולש 2
        self.vertexes.push(Vertex {
            position: top_right,
            color: c,
            uv: [1.0, -1.0],
            shape_type,
        });
        self.vertexes.push(Vertex {
            position: bottom_right,
            color: c,
            uv: [1.0, 1.0],
            shape_type,
        });
        self.vertexes.push(Vertex {
            position: bottom_left,
            color: c,
            uv: [-1.0, 1.0],
            shape_type,
        });
    }
    pub fn fill(&mut self, color: Color) {
        self.vertexes.clear();
        let mut clear_encoder =
            self.canvas
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });

        {
            let _render_pass = clear_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Hardware Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color.into()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        self.canvas
            .queue
            .submit(std::iter::once(clear_encoder.finish()));
    }

    pub fn draw_image(&mut self, x: i32, y: i32, w: u32, h: u32, image: &GPUImage) {
        // 1. קודם כל מרוקנים את כל הצורות שהצטברו עד כה בבאפר הגיאומטרי
        self.flush();

        // 2. מכינים את 4 הקודקודים של התמונה (מיקום ו-UV)
        let x = x as f32;
        let y = y as f32;
        let w = w as f32;
        let h = h as f32;

        // מערך זמני קטן של קודקודי תמונה (משולש 1 ומשולש 2)
        // הערה: הטיפוס כאן הוא סטראקט קודקוד ייעודי לתמונות או פשוט מערך פלוטים גולמי
        let image_vertices: [f32; 24] = [
            // position  // uv
            x,
            y,
            0.0,
            0.0,
            x + w,
            y,
            1.0,
            0.0,
            x,
            y + h,
            0.0,
            1.0,
            x + w,
            y,
            1.0,
            0.0,
            x + w,
            y + h,
            1.0,
            1.0,
            x,
            y + h,
            0.0,
            1.0,
        ];

        // 3. יוצרים Encoder ו-Render Pass נקודתי בשביל התמונה הזו
        let mut encoder =
            self.canvas
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Image Render Encoder"),
                });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Image Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // לא מוחקים את מה שהמלבנים ציירו!
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // 4. רינדור התמונה עם הצינור הייעודי שלה
            render_pass.set_pipeline(&self.engine.image_pipeline); // צינור נפרד לתמונות
            render_pass.set_bind_group(0, &self.engine.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &image.bind_group, &[]); // ה-Bind Group של התמונה הספציפית

            // מעלים את קודקודי התמונה לבאפר זמני או משתמשים ב-write_buffer
            // (או מחזיקים באפר ייעודי קטן לתמונות בתוך ה-Engine)
            self.canvas.queue.write_buffer(
                &self.engine.image_vertex_buffer,
                0,
                bytemuck::cast_slice(&image_vertices),
            );
            render_pass.set_vertex_buffer(0, self.engine.image_vertex_buffer.slice(..));

            render_pass.draw(0..6, 0..1);
        }

        self.canvas.queue.submit(std::iter::once(encoder.finish()));
    }
    pub fn draw_text(&mut self, text: &'b Text) {
        // <--- שים לב ללייפטיים 'b
        self.text_areas.push(glyphon::TextArea {
            buffer: &text.buffer,
            left: text.x,
            top: text.y,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: 0,
                top: 0,
                right: self.canvas.width,
                bottom: self.canvas.height,
            },
            default_color: text.color,
            custom_glyphs: &[],
        });
    }

    fn ensure_capacity(&mut self,required_vertices:usize){
        if self.vertexes.len() + required_vertices > MAX_VERTICES {
            self.flush();
        }
    }
    fn flush(&mut self) {
        if self.vertexes.is_empty() && self.text_areas.is_empty() {
            return;
        }

        self.canvas.queue.write_buffer(
            &self.engine.buffer,
            0,
            bytemuck::cast_slice(&self.vertexes),
        );

        self.engine
            .text_renderer
            .prepare(
                self.canvas.device,
                self.canvas.queue,
                &mut self.engine.font_system,
                &mut self.engine.text_atlas,
                &self.engine.text_viewport,
                self.text_areas.drain(..),
                &mut self.engine.swash_cache,
            )
            .unwrap();

        // ב. יצירת ה-Command Encoder (מקליט הפקודות)
        let mut encoder =
            self.canvas
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Canvas Render Encoder"),
                });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Canvas Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.canvas.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // ד. הקלטת פקודות הרינדור לתוך ה-Render Pass
            render_pass.set_pipeline(&self.engine.pipeline);
            render_pass.set_bind_group(0, &self.engine.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.engine.buffer.slice(..));

            // אומרים ל-GPU לצייר את כמות הקודקודים שיש לנו ב-Vec
            let num_vertices = self.vertexes.len() as u32;
            render_pass.draw(0..num_vertices, 0..1);
            self.engine
                .text_renderer
                .render(
                    &self.engine.text_atlas,
                    &self.engine.text_viewport,
                    &mut render_pass,
                )
                .unwrap();
        }

        self.canvas.queue.submit(std::iter::once(encoder.finish()));

        self.vertexes.clear();
    }


}

impl Drop for Drawer<'_, '_> {
    fn drop(&mut self) {
        self.flush();
    }
}
