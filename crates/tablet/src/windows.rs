//! Windows Ink (WM_POINTER) pen backend.
//!
//! The backend subclasses an existing window (comctl32 `SetWindowSubclass`)
//! and consumes `WM_POINTER*` messages from pens only; mouse and touch fall
//! through to the window's own procedure. Consuming pen messages also stops
//! Windows from synthesizing legacy mouse messages for them, so the
//! application never sees the same contact twice.
//!
//! For every message the full coalesced history is read with
//! `GetPointerPenInfoHistory`, so a 200+ Hz pen is not decimated to the
//! message rate. Positions come from HIMETRIC coordinates mapped through the
//! device rectangles (sub-pixel precision), validated against the integer
//! pixel position.
//!
//! `unsafe` is confined to this module: it is the FFI boundary with user32
//! and comctl32. All state touched by the window procedure lives on the UI
//! thread, so `Rc`/`RefCell` suffice and no locks are taken per sample.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use glam::Vec2;
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Controls::{
    FEEDBACK_PEN_BARRELVISUALIZATION, FEEDBACK_PEN_DOUBLETAP, FEEDBACK_PEN_PRESSANDHOLD,
    FEEDBACK_PEN_RIGHTTAP, FEEDBACK_PEN_TAP, SetWindowFeedbackSetting,
};
use windows_sys::Win32::UI::Input::Pointer::{
    GetPointerDeviceRects, GetPointerPenInfo, GetPointerPenInfoHistory, GetPointerType,
    POINTER_FLAG_CANCELED, POINTER_FLAG_DOWN, POINTER_FLAG_INCONTACT, POINTER_FLAG_UP,
    POINTER_PEN_INFO,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PEN_MASK_PRESSURE, PEN_MASK_ROTATION, PEN_MASK_TILT_X,
    PEN_MASK_TILT_Y, PT_PEN, WM_NCDESTROY, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN,
    WM_POINTERLEAVE, WM_POINTERUP, WM_POINTERUPDATE,
};

use crate::normalize::{HimetricMapper, ink_pressure, rotation_degrees, tilt_degrees};
use crate::{DeviceKind, Phase, SampleCaps, TabletEvent, TabletSample, clock};

const SUBCLASS_ID: usize = 0x7E1A_0001;
/// Upper bound on history entries read per message.
const MAX_HISTORY: usize = 256;
/// A HIMETRIC-mapped position further than this from the integer pixel
/// position is distrusted (synthetic devices, stale monitor layout).
const MAX_MAPPING_ERROR_PX: f64 = 2.0;

#[derive(Debug)]
pub enum InkError {
    Subclass,
}

impl std::fmt::Display for InkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot subclass window for pen input")
    }
}

impl std::error::Error for InkError {}

struct Shared {
    queue: RefCell<Vec<TabletEvent>>,
    waker: Box<dyn Fn()>,
    /// Per source device: mapping, or `None` once found unreliable.
    mappers: RefCell<HashMap<HANDLE, Option<HimetricMapper>>>,
    history: RefCell<Vec<POINTER_PEN_INFO>>,
    in_contact: Cell<bool>,
    samples_received: Cell<u64>,
}

/// Pen input for one window. Dropping it removes the subclass.
pub struct InkBackend {
    hwnd: HWND,
    shared: Rc<Shared>,
}

impl InkBackend {
    /// Starts capturing pen input for `hwnd`. `waker` runs (on the UI
    /// thread) whenever events become available after the queue was empty;
    /// it should schedule a [`InkBackend::drain`].
    pub fn attach(hwnd: isize, waker: impl Fn() + 'static) -> Result<Self, InkError> {
        let hwnd = hwnd as HWND;
        let shared = Rc::new(Shared {
            queue: RefCell::new(Vec::with_capacity(256)),
            waker: Box::new(waker),
            mappers: RefCell::new(HashMap::new()),
            history: RefCell::new(Vec::with_capacity(MAX_HISTORY)),
            in_contact: Cell::new(false),
            samples_received: Cell::new(0),
        });
        let raw = Rc::into_raw(shared.clone()) as usize;
        // SAFETY: `hwnd` is a live window owned by this thread; `raw` stays
        // valid until `Drop` removes the subclass and reclaims it.
        let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, raw) };
        if ok == 0 {
            // SAFETY: the subclass was not installed, so we still own `raw`.
            drop(unsafe { Rc::from_raw(raw as *const Shared) });
            return Err(InkError::Subclass);
        }
        disable_pen_feedback(hwnd);
        Ok(Self { hwnd, shared })
    }

    /// Moves all pending events into `out`.
    pub fn drain(&self, out: &mut Vec<TabletEvent>) {
        out.append(&mut self.shared.queue.borrow_mut());
    }

    /// Total pen samples received (history included), for diagnostics.
    pub fn samples_received(&self) -> u64 {
        self.shared.samples_received.get()
    }
}

impl Drop for InkBackend {
    fn drop(&mut self) {
        // SAFETY: removing our own subclass; on success Windows no longer
        // holds the raw pointer, so we reclaim the reference it carried.
        unsafe {
            if RemoveWindowSubclass(self.hwnd, Some(subclass_proc), SUBCLASS_ID) != 0 {
                drop(Rc::from_raw(Rc::as_ptr(&self.shared)));
            }
        }
    }
}

fn disable_pen_feedback(hwnd: HWND) {
    let off: i32 = 0;
    for kind in [
        FEEDBACK_PEN_BARRELVISUALIZATION,
        FEEDBACK_PEN_TAP,
        FEEDBACK_PEN_DOUBLETAP,
        FEEDBACK_PEN_PRESSANDHOLD,
        FEEDBACK_PEN_RIGHTTAP,
    ] {
        // SAFETY: `off` outlives the call; size matches a BOOL.
        unsafe {
            SetWindowFeedbackSetting(
                hwnd,
                kind,
                0,
                size_of::<i32>() as u32,
                (&off as *const i32).cast(),
            );
        }
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    refdata: usize,
) -> LRESULT {
    // SAFETY: `refdata` is the `Rc<Shared>` pointer installed in `attach`,
    // alive until the subclass is removed.
    let shared = unsafe { &*(refdata as *const Shared) };
    match msg {
        WM_POINTERDOWN
        | WM_POINTERUPDATE
        | WM_POINTERUP
        | WM_POINTERCAPTURECHANGED
        | WM_POINTERLEAVE => {
            let pointer_id = (wparam & 0xFFFF) as u32;
            let mut kind = 0;
            // SAFETY: plain out-parameter.
            let is_pen = unsafe { GetPointerType(pointer_id, &mut kind) } != 0 && kind == PT_PEN;
            if is_pen {
                handle_pen(shared, hwnd, msg, pointer_id);
                return 0;
            }
        }
        WM_NCDESTROY => {
            // The window is going away before the backend: detach so the
            // procedure is never called with a dangling `refdata`.
            // SAFETY: removing our own subclass from a live window.
            unsafe {
                if RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) != 0 {
                    drop(Rc::from_raw(refdata as *const Shared));
                }
            }
        }
        _ => {}
    }
    // SAFETY: forwarding the original arguments.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn handle_pen(shared: &Shared, hwnd: HWND, msg: u32, pointer_id: u32) {
    let was_empty = shared.queue.borrow().is_empty();
    match msg {
        WM_POINTERCAPTURECHANGED | WM_POINTERLEAVE => {
            let phase = if shared.in_contact.replace(false) {
                Phase::Cancel
            } else {
                Phase::Leave
            };
            let mut sample = TabletSample::mouse(Vec2::ZERO, clock::now());
            sample.device_kind = DeviceKind::Pen;
            shared.queue.borrow_mut().push(TabletEvent {
                phase,
                pointer_id,
                sample,
            });
        }
        _ => read_history(shared, hwnd, pointer_id),
    }
    if was_empty && !shared.queue.borrow().is_empty() {
        (shared.waker)();
    }
}

fn read_history(shared: &Shared, hwnd: HWND, pointer_id: u32) {
    // SAFETY: zeroed POD out-parameter.
    let mut current: POINTER_PEN_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: out-parameter valid for the call.
    if unsafe { GetPointerPenInfo(pointer_id, &mut current) } == 0 {
        return;
    }
    let mut history = shared.history.borrow_mut();
    let wanted = (current.pointerInfo.historyCount.max(1) as usize).min(MAX_HISTORY);
    history.clear();
    // SAFETY: POD entries, overwritten below.
    history.resize(wanted, unsafe { std::mem::zeroed() });
    let mut count = wanted as u32;
    // SAFETY: `history` has room for `count` entries.
    if unsafe { GetPointerPenInfoHistory(pointer_id, &mut count, history.as_mut_ptr()) } == 0 {
        history.clear();
        history.push(current);
    } else {
        history.truncate((count as usize).min(wanted));
    }

    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: valid window and out-parameter.
    unsafe { ClientToScreen(hwnd, &mut origin) };
    let origin = glam::DVec2::new(origin.x as f64, origin.y as f64);
    let now = clock::now();
    let newest_ms = history.first().map_or(0, |h| h.pointerInfo.dwTime);

    let mut queue = shared.queue.borrow_mut();
    // History is newest first.
    for info in history.iter().rev() {
        let pi = &info.pointerInfo;
        let flags = pi.pointerFlags;
        let phase = if flags & POINTER_FLAG_CANCELED != 0 {
            Phase::Cancel
        } else if flags & POINTER_FLAG_DOWN != 0 {
            Phase::Down
        } else if flags & POINTER_FLAG_UP != 0 {
            Phase::Up
        } else if flags & POINTER_FLAG_INCONTACT != 0 {
            Phase::Move
        } else {
            Phase::Hover
        };
        shared
            .in_contact
            .set(matches!(phase, Phase::Down | Phase::Move));

        let screen = screen_position(shared, info);
        let timestamp = if pi.PerformanceCount != 0 {
            pi.PerformanceCount as f64 / clock::qpc_frequency()
        } else {
            // Millisecond clock: place the batch relative to its arrival.
            now - newest_ms.wrapping_sub(pi.dwTime) as f64 / 1000.0
        };
        let mask = info.penMask;
        let caps = SampleCaps {
            pressure: mask & PEN_MASK_PRESSURE != 0,
            tilt: mask & (PEN_MASK_TILT_X | PEN_MASK_TILT_Y) != 0,
            rotation: mask & PEN_MASK_ROTATION != 0,
        };
        let eraser = info.penFlags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0;
        let sample = TabletSample {
            position: (screen - origin).as_vec2(),
            pressure: if caps.pressure {
                ink_pressure(info.pressure)
            } else {
                1.0
            },
            tilt_x: if mask & PEN_MASK_TILT_X != 0 {
                tilt_degrees(info.tiltX)
            } else {
                0.0
            },
            tilt_y: if mask & PEN_MASK_TILT_Y != 0 {
                tilt_degrees(info.tiltY)
            } else {
                0.0
            },
            rotation: if caps.rotation {
                rotation_degrees(info.rotation)
            } else {
                0.0
            },
            timestamp,
            device_kind: if eraser {
                DeviceKind::Eraser
            } else {
                DeviceKind::Pen
            },
            caps,
        };
        shared
            .samples_received
            .set(shared.samples_received.get() + 1);
        queue.push(TabletEvent {
            phase,
            pointer_id,
            sample,
        });
    }
}

/// Screen position in physical pixels, sub-pixel when the device mapping is
/// trustworthy.
fn screen_position(shared: &Shared, info: &POINTER_PEN_INFO) -> glam::DVec2 {
    let pi = &info.pointerInfo;
    let pixel = glam::DVec2::new(pi.ptPixelLocation.x as f64, pi.ptPixelLocation.y as f64);
    let mut mappers = shared.mappers.borrow_mut();
    let mapper = *mappers.entry(pi.sourceDevice).or_insert_with(|| {
        let mut device = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut display = device;
        // SAFETY: out-parameters valid for the call.
        let ok = unsafe { GetPointerDeviceRects(pi.sourceDevice, &mut device, &mut display) } != 0;
        ok.then(|| {
            HimetricMapper::new(
                (device.left, device.top, device.right, device.bottom),
                (display.left, display.top, display.right, display.bottom),
            )
        })
        .flatten()
    });
    match mapper {
        Some(m) => {
            let precise = m.to_screen((pi.ptHimetricLocation.x, pi.ptHimetricLocation.y));
            if (precise - pixel).length() <= MAX_MAPPING_ERROR_PX {
                precise
            } else {
                log::warn!("pen HIMETRIC mapping disagrees with pixel position; using pixels");
                mappers.insert(pi.sourceDevice, None);
                pixel
            }
        }
        None => pixel,
    }
}
