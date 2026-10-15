# `unsafe` audit

Policy:

- `unsafe_code` is **denied** workspace-wide (`[workspace.lints.rust]`). A
  module that needs it opts in with `#[allow(unsafe_code)]` at the module
  level, never crate-wide in library code.
- Every `unsafe` block and `unsafe impl` carries a `// SAFETY:` comment
  stating why the contract holds. Enforced by
  `clippy::undocumented_unsafe_blocks` (warn, and CI runs clippy with
  `-D warnings`). `unsafe_op_in_unsafe_fn` is denied.
- `unsafe` is for FFI and allocator plumbing only. No `unsafe` for speed in
  pixel loops: the hot paths are safe Rust and still allocation-free.
- Raw pointers and raw handles do not cross public APIs. Wrappers expose
  safe types (`InkBackend`, `clock::now`).

## Inventory (phase 3.5)

| Location | Kind | What | Status |
|---|---|---|---|
| `crates/tablet/src/windows.rs` | library, `#[allow(unsafe_code)]` on the module | Windows Ink backend: `SetWindowSubclass`/`RemoveWindowSubclass`, `DefSubclassProc`, `GetPointerType/Info/PenInfo/PenInfoHistory`, `GetPointerDeviceRects`, `ClientToScreen`, `SetWindowFeedbackSetting`, `Rc::into_raw/from_raw`, zero-initialized POD out-parameters | Audited; 18 blocks, all documented |
| `crates/tablet/src/clock.rs` | library, block-level `#[allow]` | `QueryPerformanceCounter`/`Frequency` out-parameters | Audited; 2 blocks |
| `crates/tablet/examples/inject_pen.rs` | example (dev tool) | Synthetic pointer device injection, window enumeration callback | Audited; every block documented |
| `crates/tablet/examples/pointer_devices.rs` | example | Device enumeration | Documented |
| `crates/corpus/examples/stamp_bench.rs` | benchmark | Counting `GlobalAlloc` forwarding to `System` | Documented |
| `crates/corpus/examples/blend_bench.rs` | benchmark | Counting `GlobalAlloc` forwarding to `System` (same as stamp_bench) | Documented |
| `crates/brush/tests/allocation.rs` | test | Counting `GlobalAlloc` (thread-local counters, const-initialized) forwarding to `System` | Documented |

No `unsafe` exists in color, canvas, brush, document, renderer, ui, corpus
(library) or app. The app reaches Windows Ink only through `InkBackend`.

## The Windows Ink backend in detail

**Ownership protocol.** `InkBackend::attach` creates `Rc<Shared>` and hands
one strong reference to Windows as the subclass `refdata`
(`Rc::into_raw`). That reference is reclaimed exactly once:

- in `Drop for InkBackend`, only if `RemoveWindowSubclass` succeeds (the
  subclass was still installed), or
- in the window procedure on `WM_NCDESTROY` (the window died first), again
  only if `RemoveWindowSubclass` succeeds.

`RemoveWindowSubclass` fails the second time, so the reference cannot be
dropped twice. If attaching fails, the reference is reclaimed immediately.
The backend keeps its own strong reference, so `Shared` outlives every
window-procedure call that can observe it.

**Handle validity.** Since phase 3.5 `attach` takes `&impl HasWindowHandle`
instead of a raw `HWND` integer: the handle is valid at the call, and
`SetWindowSubclass` itself refuses windows owned by another thread (typed
error `InkError::Subclass`). Non-Win32 windows return
`InkError::UnsupportedWindow`.

**Thread affinity.** `InkBackend` holds `Rc` and a raw `HWND`, so it is
neither `Send` nor `Sync`; the compiler keeps it on the UI thread, which is
the thread the window procedure runs on. Shared state uses `Cell`/`RefCell`;
no locks.

**Re-entrancy.** `RefCell` borrows inside the window procedure are short and
never held across calls that can pump messages. The waker
(`EventLoopProxy::send_event`) posts a message asynchronously. The app's
`drain` borrows the queue only while appending to its own vector.

**Panics.** A panic cannot unwind out of `extern "system" fn subclass_proc`
(Rust aborts). The procedure itself does not panic on bad data (failed API
calls return early); the waker must not panic (documented on `attach`).

**Out-parameters.** `POINTER_PEN_INFO`, `POINTER_INFO`, `RECT`, `POINT` are
plain C structs for which all-zero is a valid value; they are zeroed and
then filled by the API. The history buffer is resized to the requested
count before `GetPointerPenInfoHistory` writes into it, and truncated to the
count it reports.

## Checklist for new `unsafe`

1. Is there a safe alternative (a crate API, `bytemuck`, `raw-window-handle`)?
2. Confine it to a small module with `#[allow(unsafe_code)]` and a module
   comment explaining why.
3. Write `// SAFETY:` on every block: which contract, why it holds here.
4. Expose a safe API; no raw pointers in signatures.
5. Add the location to the inventory above.
