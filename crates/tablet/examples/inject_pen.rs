//! Drives a running window with a *synthetic Windows Ink pen* (pressure, tilt
//! and rotation) through the official injection API, exercising the real
//! WM_POINTER path end to end without tablet hardware.
//!
//! ```sh
//! cargo run -p tela-tablet --example inject_pen -- [window title prefix] [pattern]
//! ```
//! Patterns: `all` (default), `pressure`, `fast`, `detail`, `eraser`,
//! `drag:x0,y0,x1,y1[,pressure]` and `hold:u,v,seconds[,pressure]`
//! (client-area fractions).

#[cfg(windows)]
#[allow(unsafe_code)]
mod inject {
    use std::f32::consts::{PI, TAU};
    use std::thread::sleep;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::Controls::{
        CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, HSYNTHETICPOINTERDEVICE,
        POINTER_FEEDBACK_INDIRECT, POINTER_TYPE_INFO, POINTER_TYPE_INFO_0,
    };
    use windows_sys::Win32::UI::Input::Pointer::{
        InjectSyntheticPointerInput, POINTER_FLAG_DOWN, POINTER_FLAG_INCONTACT,
        POINTER_FLAG_INRANGE, POINTER_FLAG_PRIMARY, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
        POINTER_PEN_INFO,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowTextW, IsWindowVisible, PEN_MASK_PRESSURE,
        PEN_MASK_ROTATION, PEN_MASK_TILT_X, PEN_MASK_TILT_Y, PT_PEN, SetForegroundWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{PEN_FLAG_ERASER, PEN_FLAG_INVERTED};
    use windows_sys::core::BOOL;

    /// One pen state in client-relative unit coordinates (`0..1`).
    #[derive(Clone, Copy)]
    struct Pen {
        u: f32,
        v: f32,
        pressure: f32,
        tilt_x: i32,
        tilt_y: i32,
        rotation: u32,
        eraser: bool,
    }

    struct Injector {
        device: HSYNTHETICPOINTERDEVICE,
        origin: (i32, i32),
        size: (i32, i32),
        frame: Duration,
    }

    impl Injector {
        fn frame(&self, pen: Pen, flags: u32) {
            // SAFETY: POD initialized field by field below.
            let mut info: POINTER_PEN_INFO = unsafe { std::mem::zeroed() };
            info.pointerInfo.pointerType = PT_PEN;
            info.pointerInfo.pointerId = 0;
            info.pointerInfo.pointerFlags = flags | POINTER_FLAG_PRIMARY;
            // Optional TELA_INJECT_REGION="u0,v0,u1,v1" squeezes the patterns
            // into part of the window (e.g. beside a side panel).
            let region: Vec<f32> = std::env::var("TELA_INJECT_REGION")
                .ok()
                .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
                .filter(|r: &Vec<f32>| r.len() == 4)
                .unwrap_or_else(|| vec![0.0, 0.0, 1.0, 1.0]);
            let u = region[0] + pen.u * (region[2] - region[0]);
            let v = region[1] + pen.v * (region[3] - region[1]);
            info.pointerInfo.ptPixelLocation = POINT {
                x: self.origin.0 + (u * self.size.0 as f32).round() as i32,
                y: self.origin.1 + (v * self.size.1 as f32).round() as i32,
            };
            info.penMask =
                PEN_MASK_PRESSURE | PEN_MASK_ROTATION | PEN_MASK_TILT_X | PEN_MASK_TILT_Y;
            info.pressure = (pen.pressure.clamp(0.0, 1.0) * 1024.0) as u32;
            info.tiltX = pen.tilt_x;
            info.tiltY = pen.tilt_y;
            info.rotation = pen.rotation % 360;
            if pen.eraser {
                let contact = flags & POINTER_FLAG_INCONTACT != 0;
                info.penFlags = match std::env::var("TELA_ERASER_FLAGS").as_deref() {
                    Ok("both") => PEN_FLAG_ERASER | PEN_FLAG_INVERTED,
                    Ok("eraser") => PEN_FLAG_ERASER,
                    Ok("contact") if contact => PEN_FLAG_ERASER,
                    _ => PEN_FLAG_INVERTED,
                };
            }
            let input = POINTER_TYPE_INFO {
                r#type: PT_PEN,
                Anonymous: POINTER_TYPE_INFO_0 { penInfo: info },
            };
            // SAFETY: one valid frame for a live synthetic device.
            if unsafe { InjectSyntheticPointerInput(self.device, &input, 1) } == 0 {
                // SAFETY: reads the calling thread's last-error value; no arguments.
                let error = unsafe { GetLastError() };
                eprintln!("InjectSyntheticPointerInput failed: {error}");
            }
            sleep(self.frame);
        }

        /// Hover in, press, follow `path` (t in 0..=1), release, hover out.
        fn stroke(&self, steps: usize, path: impl Fn(f32) -> Pen) {
            let first = path(0.0);
            let hover = Pen {
                pressure: 0.0,
                ..first
            };
            self.frame(hover, POINTER_FLAG_INRANGE | POINTER_FLAG_UPDATE);
            self.frame(
                first,
                POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_DOWN,
            );
            for i in 1..=steps {
                let p = path(i as f32 / steps as f32);
                self.frame(
                    p,
                    POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_UPDATE,
                );
            }
            let last = Pen {
                pressure: 0.0,
                ..path(1.0)
            };
            self.frame(last, POINTER_FLAG_INRANGE | POINTER_FLAG_UP);
            self.frame(last, POINTER_FLAG_UPDATE);
            sleep(Duration::from_millis(60));
        }
    }

    impl Drop for Injector {
        fn drop(&mut self) {
            // SAFETY: destroying the device we created.
            unsafe { DestroySyntheticPointerDevice(self.device) };
        }
    }

    fn pen(u: f32, v: f32, pressure: f32) -> Pen {
        Pen {
            u,
            v,
            pressure,
            tilt_x: 0,
            tilt_y: 0,
            rotation: 0,
            eraser: false,
        }
    }

    /// First top-level visible window whose title starts with `prefix`.
    fn find_window(prefix: &str) -> HWND {
        struct Search {
            prefix: Vec<u16>,
            found: HWND,
        }
        unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the `Search` passed to EnumWindows below.
            let search = unsafe { &mut *(data as *mut Search) };
            let mut buf = [0u16; 512];
            // SAFETY: buffer length matches.
            let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) } as usize;
            // SAFETY: plain query.
            if unsafe { IsWindowVisible(hwnd) } != 0 && buf[..n].starts_with(&search.prefix) {
                search.found = hwnd;
                return 0;
            }
            1
        }
        let mut search = Search {
            prefix: prefix.encode_utf16().collect(),
            found: std::ptr::null_mut(),
        };
        // SAFETY: `search` outlives the enumeration.
        unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
        search.found
    }

    pub fn run(title: &str, pattern: &str) {
        let hwnd = find_window(title);
        if hwnd.is_null() {
            eprintln!("window {title:?} not found");
            std::process::exit(1);
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut origin = POINT { x: 0, y: 0 };
        // SAFETY: valid window handle and out-parameters.
        unsafe {
            SetForegroundWindow(hwnd);
            GetClientRect(hwnd, &mut rect);
            ClientToScreen(hwnd, &mut origin);
        }
        // SAFETY: plain constructor call.
        let device = unsafe { CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_INDIRECT) };
        if device.is_null() {
            // SAFETY: reads the calling thread's last-error value; no arguments.
            let error = unsafe { GetLastError() };
            eprintln!("CreateSyntheticPointerDevice failed: {error}");
            std::process::exit(1);
        }
        let inj = Injector {
            device,
            origin: (origin.x, origin.y),
            size: (rect.right, rect.bottom),
            frame: Duration::from_millis(4),
        };
        sleep(Duration::from_millis(200));
        let all = pattern == "all";

        if all || pattern == "pressure" {
            // Pressure sweep 0.05 -> 1 -> 0.05 along a straight line.
            inj.stroke(220, |t| {
                pen(0.08 + 0.84 * t, 0.12, 0.05 + 0.95 * (PI * t).sin())
            });
            // Slow S curve with tilt and rotation varying; pressure follows tilt.
            inj.stroke(260, |t| Pen {
                tilt_x: (60.0 * (TAU * t).cos()) as i32,
                tilt_y: (40.0 * (TAU * t).sin()) as i32,
                rotation: (360.0 * t) as u32,
                ..pen(0.15 + 0.12 * (TAU * t).sin(), 0.25 + 0.6 * t, 0.3 + 0.6 * t)
            });
        }
        if all || pattern == "fast" {
            // Fast circle: few samples per revolution.
            inj.stroke(28, |t| {
                pen(
                    0.45 + 0.12 * (TAU * t).cos(),
                    0.5 + 0.2 * (TAU * t).sin(),
                    0.8,
                )
            });
            // Fast diagonals with large jumps between samples.
            inj.stroke(8, |t| pen(0.62 + 0.3 * t, 0.3 + 0.55 * t, 0.9));
            inj.stroke(8, |t| pen(0.92 - 0.3 * t, 0.3 + 0.55 * t, 0.9));
            // Abrupt direction change (V) drawn quickly.
            inj.stroke(24, |t| {
                let (u, v) = if t < 0.5 {
                    (0.30 + 0.16 * t, 0.85 - 1.0 * t)
                } else {
                    (0.38 + 0.16 * (t - 0.5), 0.35 + 1.0 * (t - 0.5))
                };
                pen(u, v, 0.7)
            });
        }
        let numbers =
            |s: &str| -> Vec<f32> { s.split(',').filter_map(|v| v.trim().parse().ok()).collect() };
        if let Some(args) = pattern.strip_prefix("drag:") {
            // drag:x0,y0,x1,y1[,pressure] in client fractions.
            let a = numbers(args);
            if a.len() >= 4 {
                let p = a.get(4).copied().unwrap_or(0.7);
                inj.stroke(40, |t| {
                    pen(a[0] + (a[2] - a[0]) * t, a[1] + (a[3] - a[1]) * t, p)
                });
            }
        }
        if let Some(args) = pattern.strip_prefix("hold:") {
            // hold:u,v,seconds[,pressure]: the pen pressed and not moving.
            let a = numbers(args);
            if a.len() >= 3 {
                let p = a.get(3).copied().unwrap_or(0.8);
                let steps = (a[2] * 250.0) as usize;
                inj.stroke(steps.max(1), |_| pen(a[0], a[1], p));
            }
        }
        if pattern == "eraser" {
            // Flipped stylus: erase across what the other patterns drew.
            let rub = |u: f32, v: f32| Pen {
                eraser: true,
                ..pen(u, v, 1.0)
            };
            inj.stroke(120, |t| {
                rub(0.2 + 0.6 * t, 0.12 + 0.03 * (TAU * 3.0 * t).sin())
            });
            inj.stroke(120, |t| {
                rub(0.15 + 0.06 * (TAU * 2.0 * t).sin(), 0.2 + 0.6 * t)
            });
        }
        if all || pattern == "detail" {
            // Small spiral: fine detail.
            inj.stroke(160, |t| {
                let r = 0.002 + 0.03 * t;
                let a = 5.0 * TAU * t;
                pen(0.82 + r * a.cos(), 0.15 + r * 1.6 * a.sin(), 0.4 + 0.5 * t)
            });
        }
    }
}

#[cfg(windows)]
fn main() {
    let mut args = std::env::args().skip(1);
    let title = args.next().unwrap_or_else(|| "Mirae".into());
    let pattern = args.next().unwrap_or_else(|| "all".into());
    inject::run(&title, &pattern);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}
