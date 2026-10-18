//! The Windows calls the driver needs: the app's windows, OS input (mouse,
//! keyboard, a synthetic Windows Ink pen), screen capture. Every call
//! goes through the same paths a user's devices do (`SendInput`,
//! `InjectSyntheticPointerInput`); nothing is posted into the app's queue
//! except `WM_CLOSE`, which is what the title bar's close button sends.

#![allow(unsafe_code)]

use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, ClientToScreen, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
    SRCCOPY, SelectObject,
};
use windows_sys::Win32::UI::Controls::{
    CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, HSYNTHETICPOINTERDEVICE,
    POINTER_FEEDBACK_INDIRECT, POINTER_TYPE_INFO, POINTER_TYPE_INFO_0,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, MapVirtualKeyW, SendInput, VK_MENU,
};
use windows_sys::Win32::UI::Input::Pointer::{
    InjectSyntheticPointerInput, POINTER_FLAG_DOWN, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE,
    POINTER_FLAG_PRIMARY, POINTER_FLAG_UP, POINTER_FLAG_UPDATE, POINTER_PEN_INFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetClientRect, GetForegroundWindow, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PEN_FLAG_INVERTED, PEN_MASK_PRESSURE, PT_PEN,
    PostMessageW, SW_MAXIMIZE, SW_RESTORE, SWP_NOZORDER, SetCursorPos, SetForegroundWindow,
    SetWindowPos, ShowWindow, WM_CLOSE,
};
use windows_sys::core::BOOL;

/// A top-level window handle, as an integer so it can cross threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window(pub isize);

impl Window {
    fn hwnd(self) -> HWND {
        self.0 as HWND
    }
}

/// Physical pixels everywhere: the driver must not be DPI-virtualized.
pub fn dpi_aware() {
    // SAFETY: plain call; failing (already set) is harmless.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// A visible top-level window of process `pid`: its handle, class and title.
#[derive(Clone, Debug)]
pub struct TopLevel {
    pub window: Window,
    pub class: String,
    pub title: String,
}

/// The visible top-level windows of process `pid`.
pub fn windows_of(pid: u32) -> Vec<TopLevel> {
    struct Search {
        pid: u32,
        found: Vec<TopLevel>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `Search` passed to EnumWindows below, alive
        // for the whole enumeration.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0u32;
        // SAFETY: valid out-parameter.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        // SAFETY: plain query on a handle Windows gave us.
        if pid == search.pid && unsafe { IsWindowVisible(hwnd) } != 0 {
            let mut buf = [0u16; 512];
            // SAFETY: the buffer length is passed.
            let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
            let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            // SAFETY: the buffer length is passed.
            let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
            let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            search.found.push(TopLevel {
                window: Window(hwnd as isize),
                class,
                title,
            });
        }
        1
    }
    let mut search = Search {
        pid,
        found: Vec::new(),
    };
    // SAFETY: `search` outlives the enumeration; the callback only reads
    // through the pointer we pass.
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    search.found
}

/// The client area: its top-left corner on the screen and its size.
pub fn client(w: Window) -> ([i32; 2], [i32; 2]) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: out-parameters on the stack; a stale handle only fails.
    unsafe {
        GetClientRect(w.hwnd(), &mut rect);
        ClientToScreen(w.hwnd(), &mut origin);
    }
    (
        [origin.x, origin.y],
        [rect.right - rect.left, rect.bottom - rect.top],
    )
}

pub fn foreground() -> Window {
    // SAFETY: plain query.
    Window(unsafe { GetForegroundWindow() } as isize)
}

/// Brings `w` to the front. Windows only lets the process that has the
/// last input do that; an Alt press first gives it to us.
pub fn bring_to_front(w: Window) {
    key(VK_MENU, true);
    key(VK_MENU, false);
    // SAFETY: plain call; a stale handle only fails.
    unsafe { SetForegroundWindow(w.hwnd()) };
    sleep(Duration::from_millis(120));
}

pub fn maximize(w: Window, on: bool) {
    // SAFETY: plain call.
    unsafe { ShowWindow(w.hwnd(), if on { SW_MAXIMIZE } else { SW_RESTORE }) };
}

/// Moves and resizes the window (outer rectangle, physical pixels).
pub fn place(w: Window, x: i32, y: i32, width: i32, height: i32) {
    // SAFETY: plain call.
    unsafe {
        SetWindowPos(
            w.hwnd(),
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER,
        )
    };
}

/// What the title bar's close button sends.
pub fn close(w: Window) {
    // SAFETY: posting a message without pointers.
    unsafe { PostMessageW(w.hwnd(), WM_CLOSE, 0, 0) };
}

fn send(inputs: &[INPUT]) {
    // SAFETY: a valid slice of initialized INPUT structures.
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
}

fn mouse(flags: u32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Moves the cursor to screen pixel `(x, y)` (a real mouse move).
pub fn move_cursor(x: i32, y: i32) {
    // SAFETY: plain call.
    unsafe { SetCursorPos(x, y) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

pub fn button(b: Button, down: bool) {
    let flags = match (b, down) {
        (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (Button::Left, false) => MOUSEEVENTF_LEFTUP,
        (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
        (Button::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (Button::Middle, false) => MOUSEEVENTF_MIDDLEUP,
    };
    send(&[mouse(flags, 0)]);
}

/// Wheel notches (positive away from the user).
pub fn wheel(notches: i32) {
    send(&[mouse(MOUSEEVENTF_WHEEL, notches * 120)]);
}

/// Navigation keys are extended keys; without the flag Windows sends their
/// numpad twins (docs/AI_HANDOFF.md, 5E).
fn extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C | 0x6F)
}

/// A virtual key, pressed or released.
pub fn key(vk: u16, down: bool) {
    // SAFETY: plain call.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if down { 0 } else { KEYEVENTF_KEYUP };
    if extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    send(&[INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }]);
}

/// Types `text` as Unicode characters (for text fields and file dialogs).
pub fn type_text(text: &str) {
    for unit in text.encode_utf16() {
        for up in [false, true] {
            let flags = KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 };
            send(&[INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }]);
        }
        sleep(Duration::from_millis(4));
    }
}

/// The screen's pixels in a rectangle, as straight RGBA (opaque).
pub fn capture(x: i32, y: i32, width: i32, height: i32) -> Vec<u8> {
    let (width, height) = (width.max(1), height.max(1));
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    // SAFETY: the DCs and bitmap are created, used and released here; the
    // buffer has room for `height` rows of `width` 32-bit pixels, which is
    // what the BITMAPINFO asks GetDIBits for (top-down: negative height).
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(dc, bitmap);
        BitBlt(dc, 0, 0, width, height, screen, x, y, SRCCOPY);
        SelectObject(dc, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            bgra.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        p[3] = 255;
    }
    bgra
}

/// A synthetic Windows Ink pen (the real `WM_POINTER` path, as
/// `inject_pen` uses it; docs/testing.md "Hardware tests").
pub struct Pen {
    device: HSYNTHETICPOINTERDEVICE,
}

impl Pen {
    pub fn new() -> Option<Self> {
        // SAFETY: plain constructor call; null means failure.
        let device = unsafe { CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_INDIRECT) };
        (!device.is_null()).then_some(Self { device })
    }

    /// One pen frame at screen pixel `(x, y)`.
    fn frame(&self, x: i32, y: i32, pressure: f32, flags: u32, eraser: bool) {
        // SAFETY: a plain-data structure, initialized field by field below.
        let mut info: POINTER_PEN_INFO = unsafe { std::mem::zeroed() };
        info.pointerInfo.pointerType = PT_PEN;
        info.pointerInfo.pointerFlags = flags | POINTER_FLAG_PRIMARY;
        info.pointerInfo.ptPixelLocation = POINT { x, y };
        info.penMask = PEN_MASK_PRESSURE;
        info.pressure = (pressure.clamp(0.0, 1.0) * 1024.0) as u32;
        if eraser {
            info.penFlags = PEN_FLAG_INVERTED;
        }
        let input = POINTER_TYPE_INFO {
            r#type: PT_PEN,
            Anonymous: POINTER_TYPE_INFO_0 { penInfo: info },
        };
        // SAFETY: one valid frame for the live device we own.
        unsafe { InjectSyntheticPointerInput(self.device, &input, 1) };
        sleep(Duration::from_millis(8));
    }

    /// Hover in, press, follow the screen points, release, hover out.
    pub fn stroke(&self, points: &[[i32; 2]], pressure: f32, eraser: bool) {
        let Some((&first, rest)) = points.split_first() else {
            return;
        };
        let last = *points.last().unwrap_or(&first);
        let contact = POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT;
        self.frame(
            first[0],
            first[1],
            0.0,
            POINTER_FLAG_INRANGE | POINTER_FLAG_UPDATE,
            eraser,
        );
        self.frame(
            first[0],
            first[1],
            pressure,
            contact | POINTER_FLAG_DOWN,
            eraser,
        );
        for p in rest {
            self.frame(p[0], p[1], pressure, contact | POINTER_FLAG_UPDATE, eraser);
        }
        self.frame(
            last[0],
            last[1],
            0.0,
            POINTER_FLAG_INRANGE | POINTER_FLAG_UP,
            eraser,
        );
        self.frame(last[0], last[1], 0.0, POINTER_FLAG_UPDATE, eraser);
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        // SAFETY: destroying the device this value created, once.
        unsafe { DestroySyntheticPointerDevice(self.device) };
    }
}
