// ============================================================
// KEYBOARD HOOK
// ============================================================

use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED,
    MSLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// ============================================================
// SAFE EVENT API
// ============================================================

/// Dictates how an input event should be processed after being intercepted by the overlay.
#[derive(Eq, PartialEq, Copy, Clone)]
pub enum EventResult {
    /// The event is consumed by the overlay application. It will **not** be passed down
    /// to the underlying windows or applications (swallowed input).
    Consumed,
    /// The event is ignored or partially reacted to, allowing it to propagate normally
    /// through the OS down to target foreground applications.
    Propagated,
}

/// Identifies standard hardware mouse button mappings.
#[derive(Clone, Copy, Debug)]
pub enum MouseButton {
    /// Left mouse button.
    Left,
    /// Right mouse button.
    Right,
    /// Middle wheel click mouse button.
    Middle,
    /// Extended side button 1.
    X1,
    /// Extended side button 2.
    X2,
}

/// A unified event container representing structural asynchronous hardware input events.
#[derive(Clone, Copy, Debug)]
pub enum OverlayEvent {
    /// A keyboard button pressed state trigger.
    KeyDown {
        /// The virtual key code identifier (e.g., `VK_ESCAPE`, `0x41` for 'A').
        vk: u32,
    },

    /// A keyboard button released state trigger.
    KeyUp {
        /// The virtual key code identifier.
        vk: u32,
    },

    /// Absolute hardware cursor position motion coordinates tracking.
    MouseMove {
        /// Global desktop x-coordinate position.
        x: i32,
        /// Global desktop y-coordinate position.
        y: i32,
    },

    /// A mouse button pressed state trigger.
    MouseDown {
        /// The specific mouse button triggered.
        button: MouseButton,
    },

    /// A mouse button released state trigger.
    MouseUp {
        /// The specific mouse button released.
        button: MouseButton,
    },

    /// Vertical mouse wheel scrolling rotation delta tracker.
    MouseWheel {
        /// Rotation wheel travel step value (multiples of standard 120 units).
        delta: i16,
    },
}

pub(crate) static mut HANDLER_PTR: Handler = Handler {
    self_pointer: None,
    handler_pointer: None,
    mouse_hook: None,
    keyboard_hook: None,
};

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };

        if (kb.flags & LLKHF_INJECTED) != 0 {
            return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
        }

        let handler = std::ptr::addr_of_mut!(HANDLER_PTR);
        match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN
                if unsafe { (*handler).handle_event(OverlayEvent::KeyDown { vk: kb.vkCode }) }
                    == EventResult::Consumed =>
            {
                return 1;
            }

            WM_KEYUP | WM_SYSKEYUP
                if unsafe { (*handler).handle_event(OverlayEvent::KeyUp { vk: kb.vkCode }) }
                    == EventResult::Consumed =>
            {
                return 1;
            }

            _ => {}
        }
    }

    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

// ============================================================
// MOUSE HOOK
// ============================================================

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let mouse = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };

        if (mouse.flags & LLMHF_INJECTED) != 0 {
            return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
        }

        let handler = std::ptr::addr_of_mut!(HANDLER_PTR);
        match wparam as u32 {
            WM_MOUSEMOVE
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseMove {
                        x: mouse.pt.x,
                        y: mouse.pt.y,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_LBUTTONDOWN
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseDown {
                        button: MouseButton::Left,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_LBUTTONUP
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseUp {
                        button: MouseButton::Left,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_RBUTTONDOWN
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseDown {
                        button: MouseButton::Right,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_RBUTTONUP
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseUp {
                        button: MouseButton::Right,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_MBUTTONDOWN
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseDown {
                        button: MouseButton::Middle,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_MBUTTONUP
                if unsafe {
                    (*handler).handle_event(OverlayEvent::MouseUp {
                        button: MouseButton::Middle,
                    })
                } == EventResult::Consumed =>
            {
                return 1;
            }

            WM_MOUSEWHEEL => {
                let delta = ((mouse.mouseData >> 16) & 0xffff) as i16;

                if unsafe { (*handler).handle_event(OverlayEvent::MouseWheel { delta }) }
                    == EventResult::Consumed
                {
                    return 1;
                }
            }

            _ => {}
        }
    }

    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

pub(crate) struct Handler {
    self_pointer: Option<*mut c_void>,
    handler_pointer: Option<fn(*mut c_void, OverlayEvent) -> EventResult>,
    mouse_hook: Option<HHOOK>,
    keyboard_hook: Option<HHOOK>,
}

pub trait EventsHandler {
    fn handle_event(&mut self, event: OverlayEvent) -> EventResult;
}

impl Handler {
    pub(crate) fn register<A: EventsHandler>(&mut self, state: &mut A) {
        self.self_pointer = Some(state as *mut A as *mut c_void);

        fn trampoline_event<A: EventsHandler>(
            app_ptr: *mut c_void,
            event: OverlayEvent,
        ) -> EventResult {
            unsafe {
                let state = &mut *(app_ptr as *mut A);
                state.handle_event(event)
            }
        }

        self.handler_pointer = Some(trampoline_event::<A>);
    }

    pub(crate) fn handle_event(&mut self, event: OverlayEvent) -> EventResult {
        // קריאה ישירה ומהירה למצביע הפונקציה השמור
        if let (Some(handler), Some(state)) = (self.handler_pointer, self.self_pointer) {
            handler(state, event)
        } else {
            EventResult::Propagated
        }
    }

    pub(crate) fn start(&mut self) {
        let hinstance = unsafe { GetModuleHandleW(null()) };

        if hinstance.is_null() {
            return;
        }
        unsafe {
            self.keyboard_hook = Some(SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                hinstance as HINSTANCE,
                0,
            ));

            self.mouse_hook = Some(SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_hook_proc),
                hinstance as HINSTANCE,
                0,
            ));
        }
    }

    pub(crate) fn stop(&mut self) {
        if let Some(h) = self.mouse_hook {
            unsafe {
                UnhookWindowsHookEx(h);
            }
        }

        if let Some(h) = self.keyboard_hook {
            unsafe {
                UnhookWindowsHookEx(h);
            }
        }
    }
}

/// Queries the dynamic worldwide hardware desktop coordinate cursor tracking position.
pub fn mouse_position() -> (i32, i32) {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };

        GetCursorPos(&mut pt);

        (pt.x, pt.y)
    }
}

/// Synthesizes and injects an asynchronous hardware input event into the OS stream.
///
/// This translates the high-level `OverlayEvent` representation into raw, serialized
/// structural input payloads. The resulting operations are automatically flagged as injected,
/// instructing internal low-level event hooks to bypass tracking and prevent operational deadlocks.
///
/// # Arguments
/// * `event` - A reference to the structural `OverlayEvent` targeted for system injection.
pub fn send_event(event: &OverlayEvent) {
    unsafe {
        let mut inputs: Vec<INPUT> = Vec::new();

        match event {
            OverlayEvent::KeyDown { vk } => {
                let mut input = zeroed::<INPUT>();
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki = KEYBDINPUT {
                    wVk: *vk as u16,
                    wScan: 0,
                    dwFlags: 0,
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            OverlayEvent::KeyUp { vk } => {
                let mut input = zeroed::<INPUT>();
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki = KEYBDINPUT {
                    wVk: *vk as u16,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            OverlayEvent::MouseMove { x, y } => {
                let mut input = zeroed::<INPUT>();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi = MOUSEINPUT {
                    dx: *x,
                    dy: *y,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            OverlayEvent::MouseDown { button } => {
                let mut input = zeroed::<INPUT>();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi = MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: match button {
                        MouseButton::X1 => 0x0001,
                        MouseButton::X2 => 0x0002,
                        _ => 0,
                    },
                    dwFlags: match button {
                        MouseButton::Left => MOUSEEVENTF_LEFTDOWN,
                        MouseButton::Right => MOUSEEVENTF_RIGHTDOWN,
                        MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN,
                        MouseButton::X1 | MouseButton::X2 => MOUSEEVENTF_XDOWN,
                    },
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            OverlayEvent::MouseUp { button } => {
                let mut input = zeroed::<INPUT>();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi = MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: match button {
                        MouseButton::X1 => 0x0001,
                        MouseButton::X2 => 0x0002,
                        _ => 0,
                    },
                    dwFlags: match button {
                        MouseButton::Left => MOUSEEVENTF_LEFTUP,
                        MouseButton::Right => MOUSEEVENTF_RIGHTUP,
                        MouseButton::Middle => MOUSEEVENTF_MIDDLEUP,
                        MouseButton::X1 | MouseButton::X2 => MOUSEEVENTF_XUP,
                    },
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            OverlayEvent::MouseWheel { delta } => {
                let mut input = zeroed::<INPUT>();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi = MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: (*delta as u32) << 16,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }
        }

        if !inputs.is_empty() {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                size_of::<INPUT>() as i32,
            );
        }
    }
}
