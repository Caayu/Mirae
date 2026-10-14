# Concurrency

## Threads

| Thread | Created by | Runs | Shares with others |
|---|---|---|---|
| Main / UI thread | the OS | winit event loop, Windows Ink window procedure (subclass), brush pipeline, composition, GPU submission, egui | nothing mutable |
| rayon global pool (one worker per hardware thread) | rayon, lazily on first parallel batch | per-tile dab stamping of large batches | tiles moved in and out by value; read-only `StampContext` and dab slice |
| wgpu / driver threads | wgpu and the GPU driver | internal | opaque to Tela |

There are no other threads, no channels, and no `Arc<Mutex<_>>` anywhere.
`InkBackend` is `!Send` (it holds `Rc` and a raw `HWND`), so the compiler
keeps the pen backend on the UI thread; its state uses `Cell`/`RefCell`.

## Ownership on the main thread

`App` owns everything (document, view, engine, tools, renderer, UI). Each
winit callback runs to completion; pen samples queued by the window
procedure are drained in `App::drain_tablet`, so input processing never
races with rendering. Composition runs on the main thread, per dirty tile,
into reusable buffers.

## Parallel stamping

`StrokeBuffer` stamps each batch of dabs (all dabs produced by one input
sample, or one tail rebuild) with `stamp_batch` (`crates/brush/src/stroke.rs`):

1. Estimate the work: sum of the dabs' bounding-box areas in pixels.
2. If `!ctx.parallel` or work `< PARALLEL_MIN_WORK` (60 000 px): stamp
   sequentially, dab by dab.
3. Otherwise: list `(tile, dab index)` pairs, sort them (tile, then dab
   order), **move** each affected tile out of the surface into a job, run
   the jobs with `par_iter_mut` (each job owns its tile exclusively and
   applies its dabs in stroke order), then move the tiles back.

No locks and no atomics are involved in Tela's code: each tile has exactly
one owner at a time, the dab list and `StampContext` (tip, texture) are
shared read-only. (rayon itself uses lock-free queues; submitting a batch
from the UI thread allocates one queue block per 63 batches, see
performance.md.)

### Determinism

The per-pixel update is order-dependent
(`m += (target − m)·flow·cov`). Splitting by tile keeps, for every pixel,
the same sequence of dabs in the same order as sequential stamping, and
floating-point operations per pixel are identical. Therefore the result is
**bit-identical** (test `parallel_stamping_is_bit_identical_to_sequential`;
goldens are rendered with parallel stamping enabled).

### Why small brushes stay single-threaded

A batch typically holds the dabs of one input sample: a pen at 2000 px/s
sampled at 200 Hz moves 10 px per sample, i.e. ~20 dabs of a 5 px brush or
one dab of a 100 px brush. Such a batch is a few thousand to ~10 000 pixels,
which takes microseconds on one core; dispatching to rayon, sorting pairs
and moving tiles costs more than it saves. Measured (docs/performance.md):
5–100 px brushes run at 0.99–1.01× with parallelism enabled because they
stay below the threshold, while 250 px runs ~1.7× and 500 px ~3× faster.
The threshold (60 000 px of estimated work) was chosen from that benchmark;
change it only with new measurements.

Parallelism is also bounded by the number of tiles a batch touches (a
500 px dab touches ~9 tiles), which is why the speed-up is ~3×, not 16×.

## Rules

- Do not introduce shared mutable state across threads; move ownership
  instead (the tile-job pattern).
- Anything that runs on rayon must be deterministic per pixel regardless of
  scheduling.
- Do not block the UI thread on I/O; there is none yet (document I/O in
  phase 4 will need a policy).
