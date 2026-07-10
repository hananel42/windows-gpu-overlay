// -------------------
//  Made using AI.
// -------------------
use real_gpu_app::canvas::{Simple2DEngine, Text};
use real_gpu_app::hooks::{EventResult, OverlayEvent};
use real_gpu_app::{run, Canvas, OverlayContext, OverlayGPUApp};

struct SandboxParticle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    base_color: (f32, f32, f32),
}

struct MyApp {
    engine: Option<Simple2DEngine>,
    status_text: Option<Text>,

    // מצב האפליקציה (State)
    particles: Vec<SandboxParticle>,
    well_x: f32,
    well_y: f32,
    gravity: f32,
    paused: bool,
    color_scheme: u8, // 0 = אש, 1 = ניאון, 2 = קרח

    // חישובי פעימה וזמן
    pulse_timer: f32,
    core_angle: f32,
    fps_timer: f32,
    frame_count: u32,
    current_fps: u32,
}

impl OverlayGPUApp for MyApp {

    // 1. INIT: אתחול המנוע, החלקיקים ומצב התחלתי
    fn init(&mut self, context: &mut OverlayContext) {
        let mut engine = Simple2DEngine::new(context);

        let center_x = (context.width() / 2) as f32;
        let center_y = (context.height() / 2) as f32;

        // יצירת טקסט הסטטוס (יעודכן ב-update)
        let t = engine.text("Initializing...", 20.0, 20.0, 24.0, (255, 255, 255, 255));

        self.engine = Some(engine);
        self.status_text = Some(t);
        self.well_x = center_x;
        self.well_y = center_y;

        // ייצור חלקיקים ראשוני
        self.reset_particles(context.width() as f32, context.height() as f32);

        println!("[API TEST] Init Completed Successfully.");
    }

    // 2. HANDLER: בדיקת מערכת הקלט וניהול אירועים מקיף
    fn handler(&mut self, context: &mut OverlayContext, event: OverlayEvent) -> EventResult {
        match event {
            OverlayEvent::KeyDown { vk } => match vk {
                27 => { // ESC - סגירה
                    context.close().unwrap();
                    EventResult::Consumed
                }
                32 => { // SPACE - השהייה
                    self.paused = !self.paused;
                    EventResult::Consumed
                }
                38 => { // חץ למעלה - הגברת כבידה
                    self.gravity = (self.gravity + 500.0).min(10000.0);
                    EventResult::Consumed
                }
                40 => { // חץ למטה - החלשת כבידה
                    self.gravity = (self.gravity - 500.0).max(-2000.0);
                    EventResult::Consumed
                }
                37 => { // חץ שמאלה - הזזת הליבה שמאלה
                    self.well_x -= 30.0;
                    EventResult::Consumed
                }
                39 => { // חץ ימינה - הזזת הליבה ימינה
                    self.well_x += 30.0;
                    EventResult::Consumed
                }
                82 => { // R - איפוס מיקומי חלקיקים
                    self.reset_particles(context.width() as f32, context.height() as f32);
                    EventResult::Consumed
                }
                67 => { // C - החלפת פלטת צבעים
                    self.color_scheme = (self.color_scheme + 1) % 3;
                    self.update_particle_colors();
                    EventResult::Consumed
                }
                _ => EventResult::Propagated,
            },
            _ => EventResult::Propagated,
        }
    }

    // 3. UPDATE: חישובי פיזיקה, מתמטיקה ועדכון טקסטים
    fn update(&mut self, context: &mut OverlayContext, delta: f32) {
        // א) חישוב FPS בזמן אמת ללא ספריות חיצוניות
        self.fps_timer += delta;
        self.frame_count += 1;
        if self.fps_timer >= 1.0 {
            self.current_fps = self.frame_count;
            self.frame_count = 0;
            self.fps_timer -= 1.0;
        }

        // ב) עדכון טקסט הסטטוס (בדיקת יעילות ה-Update של Glyphon)
        let status_string = format!(
            "FPS: {} | Particles: {} | Gravity: {} | Scheme: {} | Space: {}",
            self.current_fps,
            self.particles.len(),
            self.gravity,
            match self.color_scheme { 0 => "Fire", 1 => "Neon", _ => "Ice" },
            if self.paused { "PAUSED" } else { "RUNNING" }
        );

        let engine_mut = self.engine.as_mut().unwrap();
        self.status_text.as_mut().unwrap().update(engine_mut, &status_string);

        // ג) אנימציית סיבוב ופעימה של הליבה הגרפית
        self.pulse_timer += delta * 5.0;
        self.core_angle += delta * 2.0;

        // ד) חישובי פיזיקה חלקיקית (רץ רק אם לא ב-Pause)
        if !self.paused {
            let width = context.width() as f32;
            let height = context.height() as f32;

            for p in &mut self.particles {
                // חישוב וקטור המרחק מהליבה המגנטית
                let dx = self.well_x - p.x;
                let dy = self.well_y - p.y;
                let distance_sq = dx * dx + dy * dy + 1000.0; // סף הגנה נגד חלוקה באפס
                let distance = distance_sq.sqrt();

                // חוק הגרביטציה: הכוח יחסי הפוך למרחק
                let force = (self.gravity * 10.0) / distance_sq;

                // עדכון מהירויות
                p.vx += (dx / distance) * force * delta * 60.0;
                p.vy += (dy / distance) * force * delta * 60.0;

                // חיכוך קל (Drag) כדי למנוע מהירות אינסופית
                p.vx *= 0.98;
                p.vy *= 0.98;

                // עדכון מיקום
                p.x += p.vx * delta * 60.0;
                p.y += p.vy * delta * 60.0;

                // החזרת חלקיקים שבורחים מהמסך מהצד השני (Screen Wrapping)
                if p.x < 0.0 { p.x = width; }
                if p.x > width { p.x = 0.0; }
                if p.y < 0.0 { p.y = height; }
                if p.y > height { p.y = 0.0; }
            }
        }
    }

    // 4. RENDER: ציור מורכב הבודק את גבולות ה-Batching של המנוע
    fn render(&mut self, canvas: Canvas) {
        let engine = self.engine.as_mut().unwrap();
        let mut drawer = engine.drawer(&canvas);

        // א) ציור לוח ה-HUD האחורי (מלבן שחור שקוף למעלה)
        drawer.draw_rect(10, 10, 650, 45, (0.0, 0.0, 0.0, 180.0).into());

        // ב) ציור הליבה המגנטית (עיגול פועם + משולש מסתובב במרכז הליבה)
        let core_pulse_radius = 25 + (self.pulse_timer.sin() * 8.0) as u32;
        let core_color = match self.color_scheme {
            0 => (255.0, 100.0, 0.0, 150.0), // אש
            1 => (255.0, 0.0, 255.0, 150.0), // ניאון
            _ => (0.0, 200.0, 255.0, 150.0), // קרח
        }.into();

        drawer.draw_circle(self.well_x as i32, self.well_y as i32, core_pulse_radius, core_color);

        // חישוב קודקודי משולש מסתובב בתוך הליבה
        let tri_size = 15.0;
        let p1 = [
            (self.well_x + self.core_angle.cos() * tri_size) as i32,
            (self.well_y + self.core_angle.sin() * tri_size) as i32,
        ];
        let p2 = [
            (self.well_x + (self.core_angle + 2.094).cos() * tri_size) as i32,
            (self.well_y + (self.core_angle + 2.094).sin() * tri_size) as i32,
        ];
        let p3 = [
            (self.well_x + (self.core_angle + 4.188).cos() * tri_size) as i32,
            (self.well_y + (self.core_angle + 4.188).sin() * tri_size) as i32,
        ];
        drawer.draw_triangle(p1, p2, p3, (255.0, 255.0, 255.0, 255.0).into());

        // ג) ציור מאות חלקיקים על המסך במכה אחת
        for p in &self.particles {
            let color = (p.base_color.0, p.base_color.1, p.base_color.2, 200.0).into();
            // שימוש בעיגולים קטנים (בגודל 2 פיקסלים)
            drawer.draw_circle(p.x as i32, p.y as i32, 2, color);
        }

        // ד) ציור טקסט הסטטוס (Batching יעיל)
        drawer.draw_text(self.status_text.as_ref().unwrap());
    }

    // 5. SHUTDOWN: בדיקת שלב סגירת האפליקציה וניקוי המשאבים
    fn shutdown(&mut self, _context: &mut OverlayContext) {
        println!("[API TEST] Shutdown called. Releasing resources, saving configuration... Goodbye!");
    }
}

// פונקציות עזר פנימיות לניהול המצב ב-MyApp
impl MyApp {
    fn reset_particles(&mut self, max_w: f32, max_h: f32) {
        let mut particles = Vec::new();
        // נתחיל עם 350 חלקיקים כדי לאמץ את הצינור הגרפי
        for i in 0..20000 {
            let x = (i * 17) as f32 % max_w;
            let y = (i * 23) as f32 % max_h;
            particles.push(SandboxParticle {
                x,
                y,
                vx: 0.0,
                vy: 0.0,
                base_color: (0.0, 0.0, 0.0),
            });
        }
        self.particles = particles;
        self.update_particle_colors();
    }

    fn update_particle_colors(&mut self) {
        for (i, p) in self.particles.iter_mut().enumerate() {
            p.base_color = match self.color_scheme {
                0 => (255.0, 50.0 + (i % 150) as f32, 0.0), // פלטת אש
                1 => (50.0 + (i % 200) as f32, 255.0, 50.0), // פלטת ניאון
                _ => (0.0, 150.0 + (i % 100) as f32, 255.0), // פלטת קרח
            };
        }
    }
}

fn main() {
    let mut app = MyApp {
        engine: None,
        status_text: None,
        particles: Vec::new(),
        well_x: 0.0,
        well_y: 0.0,
        gravity: 2500.0, // עוצמת משיכה התחלתית בריאה
        paused: false,
        color_scheme: 0,
        pulse_timer: 0.0,
        core_angle: 0.0,
        fps_timer: 0.0,
        frame_count: 0,
        current_fps: 0,
    };
    run(&mut app).expect("Sandbox run failure");
}