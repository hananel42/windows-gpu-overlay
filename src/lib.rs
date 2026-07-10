pub mod canvas;
pub mod hooks;
pub mod screen_capture;

use std::mem::zeroed;
use std::time::Instant;

use wgpu::{CurrentSurfaceTexture, Device, Queue, Surface, TextureView};

pub use crate::canvas::Canvas;
use crate::hooks::*;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::CreateDXGIFactory1;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetSystemMetrics, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, PeekMessageW,
    PostQuitMessage, RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SetProcessDPIAware,
    SetWindowDisplayAffinity, ShowWindow, TranslateMessage, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST, WM_QUIT, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, Interface};

// ============================================================
// WINDOWS PROCEDURE
// ============================================================
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(-1), // Click-through חומרתי מלא
        WM_ERASEBKGND => LRESULT(1),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

// ============================================================
// OVERLAY CONTEXT
// ============================================================
pub struct OverlayContext {
    hwnd: HWND,
    width: i32,
    height: i32,
    surface: Surface<'static>,
    comp_device: IDCompositionDevice,
    _comp_visual: IDCompositionVisual,
    comp_target: IDCompositionTarget,
    device: Device,
    queue: Queue,
    format: wgpu::TextureFormat,
}

impl OverlayContext {
    fn new() -> Option<OverlayContext> {
        let hinstance = unsafe { GetModuleHandleW(None) }.ok()?;
        let class_name = windows::core::w!("WgpuOverlayClass");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.ok()?,
            ..Default::default()
        };
        unsafe { RegisterClassW(&wc) };

        let width =
            unsafe { GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN) };
        let height =
            unsafe { GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN) };

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP,
                class_name,
                windows::core::w!("Transparent WGPU Overlay"),
                WS_POPUP,
                0,
                0,
                width,
                height, // מסך מלא מבטיח תאימות קומפוזיציה
                None,
                None,
                Some(HINSTANCE::from(hinstance)),
                None,
            )
        }
        .ok()?;

        // 2. בניית אובייקטי ה-DirectComposition
        let _dxgi_factory: windows::Win32::Graphics::Dxgi::IDXGIFactory1 =
            unsafe { CreateDXGIFactory1() }.ok()?;
        let comp_device: IDCompositionDevice = unsafe { DCompositionCreateDevice(None) }.ok()?;
        let comp_target: IDCompositionTarget =
            unsafe { comp_device.CreateTargetForHwnd(hwnd, true) }.ok()?;
        let comp_visual: IDCompositionVisual = unsafe { comp_device.CreateVisual() }.ok()?;
        unsafe { comp_target.SetRoot(&comp_visual) }.ok()?;

        // 3. אתחול מנוע WGPU
        let visual_ptr = comp_visual.as_raw();
        let instance = wgpu::Instance::default();
        let surface_target = wgpu::SurfaceTargetUnsafe::CompositionVisual(visual_ptr);
        let surface = unsafe { instance.create_surface_unsafe(surface_target) }.ok()?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .unwrap();

        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();

        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: Default::default(),
            width: width as u32,   // גודל דינמי לפי המסך
            height: height as u32, // גודל דינמי לפי המסך
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        unsafe { comp_device.Commit().ok() }?;

        Some(OverlayContext {
            hwnd,
            width,
            height,
            surface,
            comp_device,
            _comp_visual: comp_visual,
            comp_target,
            device,
            queue,
            format,
        })
    }

    #[inline(always)]
    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn show(&mut self) {
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
    }

    pub fn hide(&mut self) {
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    #[inline(always)]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    #[inline(always)]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn close(&mut self) -> Result<(), Error> {
        unsafe {
            DestroyWindow(self.hwnd)?;
        }
        Ok(())
    }

    pub fn width(&self) -> i32 {
        self.width
    }
    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn mouse_position(&self) -> (i32, i32) {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut pt);
            (pt.x, pt.y)
        }
    }

    pub fn hide_from_capture(&mut self, hide: bool) {
        unsafe {
            if hide {
                let _ = SetWindowDisplayAffinity(self.hwnd, WDA_EXCLUDEFROMCAPTURE);
            } else {
                let _ = SetWindowDisplayAffinity(self.hwnd, WDA_NONE);
            }
        }
    }
}

// ============================================================
// SAFE EVENT API & APP TRAIT
// ============================================================
pub trait OverlayGPUApp {
    fn init(&mut self, _context: &mut OverlayContext) {}
    fn handler(&mut self, _context: &mut OverlayContext, _event: OverlayEvent) -> EventResult {
        EventResult::Propagated
    }
    fn update(&mut self, _context: &mut OverlayContext, _delta: f32) {}
    fn render(&mut self, _canvas: Canvas) {}
    fn shutdown(&mut self, _context: &mut OverlayContext) {}
}

struct AppWrapper<'a, A: OverlayGPUApp> {
    app: &'a mut A,
    context: OverlayContext,
}
impl<A: OverlayGPUApp> EventsHandler for AppWrapper<'_, A> {
    fn handle_event(&mut self, event: OverlayEvent) -> EventResult {
        self.app.handler(&mut self.context, event)
    }
}

// ============================================================
// ENGINE RUNTIME ENTRY
// ============================================================
pub fn run<A: OverlayGPUApp>(app: &mut A) -> Result<(), String> {
    unsafe {
        let _ = SetProcessDPIAware();
    }
    let context = match OverlayContext::new() {
        None => return Err(String::from("window initialization failed")),
        Some(context) => context,
    };

    let mut wrapper = AppWrapper { app, context };

    let handler_ptr = unsafe { &mut *std::ptr::addr_of_mut!(HANDLER_PTR) };
    handler_ptr.register(&mut wrapper);
    handler_ptr.start();

    wrapper.app.init(&mut wrapper.context);
    wrapper.app.update(&mut wrapper.context, 0.0);
    wrapper.context.show();

    let mut msg: MSG = unsafe { zeroed() };
    let mut last = Instant::now();

    'a: loop {
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                break 'a;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let now = Instant::now();
        let delta = now.duration_since(last).as_secs_f32();
        last = now;

        wrapper.app.update(&mut wrapper.context, delta);

        let current_surface_texture = match wrapper.context.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            _ => return Err(String::from("Failed to get texture")),
        };

        let view = current_surface_texture
            .texture
            .create_view(&Default::default());
        let canvas = Canvas {
            device: &wrapper.context.device,
            queue: &wrapper.context.queue,
            view,
            width: wrapper.context.width,
            height: wrapper.context.height,
        };
        wrapper.app.render(canvas);

        wrapper.context.queue.present(current_surface_texture);
        unsafe {
            wrapper
                .context
                .comp_device
                .Commit()
                .map_err(|_| String::from("Failed to commit comp device"))?;
        }
    }

    handler_ptr.stop();
    wrapper.app.shutdown(&mut wrapper.context);

    Ok(())
}
