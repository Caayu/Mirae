# Technical debt

Known, real debt, **updated at the end of phase 5A**. Each item:
impact, why it is not fixed now, and a possible direction. Priorities:
**P1** blocks a planned phase or affects users soon; **P2** visible quality
issue; **P3** latent or cosmetic. Item numbers are stable (other documents
refer to them); resolved items are listed at the end, ideas without a
defect live in [ROADMAP.md](ROADMAP.md).

## P1

| # | Item | Impact | Why not now | Direction |
|---|---|---|---|---|
| 1 | **No physical tablet validated.** Only a virtual HID pen exists on the dev machine; tests use synthetic Windows Ink injection with integer positions. | Real pens may differ in pressure response, tilt/rotation, timestamps, delivery batching; HIMETRIC sub-pixel mapping is untested; mask pressure not comparable with the reference (D-4D-1). | No hardware available. | Test on at least one Wacom-class and one N-trig/Surface pen; record device/driver/results in STATUS.md; add Wintab if Ink proves insufficient. |
| 20 | **Blending over a translucent backdrop without paper differs from the reference** (parity D-4C-1, D-4D-6): Mirae follows W3C, the reference does not (additive alpha in some modes, colours up to ~25/255 off). | None on screen (the paper is always under the stack); wrong results for transparent export (refused, D-4F-5), folders and merge down. | Rule unpublished; not reachable yet; measured data kept (`tests/reference/blend_without_paper.csv`). | Dedicated probe of translucent backdrops; fit a model; a parity test like `blend_parity.rs`. **Prerequisite of phase 5B** (merge down) and of folders. |

## P2

| # | Item | Impact | Why not now | Direction |
|---|---|---|---|---|
| 3 | **Document limited by GPU max texture size** (single display texture). | Documents beyond 8192/16384 px cannot be shown. | Typical illustration sizes fit. | Page the display texture (tiles of display textures) while keeping mip continuity. |
| 8 | **UI edits are not persisted**; preset changes are lost on exit (documents are, since 4F). | Users cannot keep customized brushes. | Presets are not stored in documents (D19). | Save user presets as JSON (format exists) in a user directory; "reset to built-in". |
| 17 | **Layer panel is minimal**: no drag-and-drop reorder, no rename field, no thumbnails, no delete confirmation (the reference asks; in Mirae deletion is undoable). The right panel's width follows its content (status line, long rows such as "[editing mask]"), so controls move (seen in the 4G app session, and again in 5A: "[editing mask]" widens the panel and moves its buttons). | Less familiar than the reference; controls shift under the cursor. | 4A–4F scope was the model, history and files. | Fixed panel width; drag reorder, rename and thumbnails (phase 5B); decide on a confirmation with the owner. |
| 18 | **`commit_tile` converts whole tiles** (all 256 rows, stored ↔ f32) even where the stroke mask is empty. | ~0.6 ms per touched tile at commit (`history_bench`, `doc_bench`: a 50-tile stroke commits in ~47 ms). | Pre-existing; not in phase 4's scope. | Skip rows (or tiles) whose mask is all zero; re-run `history_bench` and the goldens. |
| 21 | **No "merge down"** (and no merge visible / flatten; D-4C-3). | Workflow gap vs the reference. | Depends on #20 for translucent lower layers. | After #20, as one undo step (tiles of the lower layer + removal of the upper). |
| 23 | **Masks: no apply-to-layer, no mask from selection, no mask display mode, no unlinking** (D-4D-2). | Workflow gaps vs the reference. | Apply needs a recorded merge of mask into pixels; selections do not exist yet. | Apply as one history step; masks from selections with the selection subphase (5C). |
| 28 | **Saving and loading block the UI thread.** | A full 16-bit 4096² × 10 document takes ~6 s to save (deflate-bound), 8-bit ~2 s (`io_bench`); 100 sparse 16-bit layers ~8 s (`doc_bench`). | 4F scope: synchronous, measured first. | Save in the background from a snapshot (tiles are swappable), and/or deflate tiles on rayon and write them in order; same serializer. |
| 29 | **No autosave, recent files, revert or save copy** (D-4F-8). | A crash loses unsaved work; fewer conveniences than the reference. | Not in 4F. | Autosave with `write_document` to a recovery file when modified (after #28); a recent-files list in user settings. |
| 32 | **Imported images are plain raster layers**, placed centred, not transformable (D-4F-7); PNGs cannot be opened as documents (D-4F-8). | Placement must be painted around; no "open PNG". | Transform tools are a later subphase (5D). | Open a PNG as a new document of its size; image layers with a pending transform once transforms exist. |
| 33 | **CPU composition scales linearly with layers, single-threaded, with no cache below the stroke target.** Measured in 4G (`doc_bench`, 8-bit, per 256² tile, every layer drawing): Normal 0.37 ms (1 layer) → 12.4 ms (100); Multiply 29.7 ms and Soft Light 44.4 ms at 100; per layer per tile ≈ decode 64 µs + Normal 65 µs (Multiply 351, Soft Light 532). A 4-tile stroke frame on 100 sparse layers costs ~5 ms, on 50 partial layers ~11 ms. | Stroke frames on documents with many overlapping or non-Normal layers exceed a 60 Hz budget (16.7 ms) at about 100 Normal or 40 Soft Light layers covering the stroke. | Correctness and determinism first; typical documents (≤ 50 layers, mostly Normal) stay inside the budget; optimizing was out of 4G's scope. | In order of expected gain: cache the composite below the stroke target during a stroke (invalidated on any other change); compose dirty tiles in parallel (deterministic per tile); fuse decode with Normal blending; GPU composition later (decision in the phase-5 proposal). Re-run `doc_bench` and the parity tests. |

## P3

| # | Item | Impact | Why not now | Direction |
|---|---|---|---|---|
| 5 | **Airbrush rest position lags one sample** (uses the last processed path point). | Slight offset of rest build-up after a fast stop. | Visually negligible at 200 Hz. | Use the raw last input position for rest deposition, filtered for jitter. |
| 6 | **`distance` effector uses a fixed period `length`**, not the stroke's final length (unknown while drawing). | "Fade over the whole stroke" is not expressible. | Needs a design for live strokes. | Optional "fit to stroke" mode evaluated in the provisional tail, like the end taper. |
| 9 | **Goldens verified on one machine only.** | Different CPUs/compilers might exceed tolerances via float differences. | No second platform available. | Run the corpus in CI on another OS/CPU; widen tolerances only with evidence. |
| 11 | **Embedded tip images are JSON number arrays.** | Presets with image tips are ~4× larger than necessary. | No user files exist yet. | Base64 in preset format v2 with a v1→v2 migration. |
| 12 | **`missing_docs` not enforced**; ~197 items (measure: `RUSTFLAGS="-W missing_docs" cargo check --workspace --lib`) lack doc comments. | New public API may go undocumented. | Enforcing now would add noise docs. | Enable `missing_docs` per crate as each crate's API stabilizes (color, canvas first). |
| 13 | **Curved interpolation runs one sample behind**; stroke ends use a mirrored phantom neighbour. | ~4 ms extra latency at 240 Hz; slightly flatter last segment. | Inherent to the chosen spline. | Combine with movement prediction (ROADMAP). |
| 14 | **Pixels painted outside the document are kept** in layers (clipped at composition). | Memory for off-canvas paint; surprising on a future canvas resize. | Harmless today. | Clip at commit, or define canvas-resize semantics with the resize feature. |
| 15 | **Mouse samples have no device timestamps** (stamped on arrival). | Velocity-driven effectors are noisier with a mouse. | winit does not expose them. | Use raw input timestamps on Windows if needed. |
| 16 | **Large modules**: `brush/src/engine.rs` (~800 lines incl. tests) holds `StrokeProcessor` and `BrushEngine`. | Harder to navigate. | Cohesive (one pipeline); splitting now is cosmetic. | Split `BrushEngine` into its own module if it grows. |
| 16a | **Parallel stamping allocates inside rayon** (one injector block per 63 batches) and dirty sets reallocate `BTreeSet` nodes per frame. | Negligible (measured, performance.md), but not literally zero. | Needs a custom submission path or a non-allocating dirty set. | A persistent bitset/Vec-backed dirty set; submit via a pool-resident task if it ever matters. |
| 19 | **Undo history is uncompressed and in memory only.** | 256 KiB per changed tile per step (512 KiB in 16-bit documents); 1 GiB holds ~55 strokes of a 500 px brush across a large canvas. | ADR 0015: no compression. | Compress old steps or spill to disk if measurements show the need. |
| 24 | **Deleting a mask does not ask whether to apply it** (D-4D-3). | Less familiar; nothing is lost (undoable). | Depends on #23 (apply). | Add the question once apply exists; decide with the owner. |
| 25 | **Masks are 8-bit in 16-bit documents** (D13; D-4E-4). | Repeated faint mask strokes may band like 8-bit pixels; not measured. | Approved for phase 4; no measurement yet. | Measure mask accumulation with `banding`; 16-bit masks would double mask memory (64 → 128 KiB per tile). |
| 26 | **No dithering and an 8-bit display** at every storage depth. | 16-bit documents show 8-bit levels on screen; storage keeps the precision. | Excluded from 4E. | Optional display dithering in the shader, or a 10-bit swapchain; never in stored pixels. |
| 30 | **Unknown manifest fields from newer compatible writers are dropped on re-save** (warned at load). | Round-tripping a newer file through an older Mirae loses that information. | v1 has no such fields yet. | Keep unknown JSON values per object and write them back, or bump the version when such fields matter. |
| 31 | **PNG metadata**: no ICC profile, no resolution, white vs black under zero alpha (D-4F-6). | Other apps assume sRGB from the `sRGB` chunk; print sizes are unknown. | No colour management or document resolution yet. | Embed an sRGB profile and a resolution once documents have them. |
| 34 | **A history step larger than the budget is kept alone** (invariant H4), so converting a large document's depth holds both copies: 100 dense 4096² layers 8 → 16 bits peak at ~3× the 8-bit size (6.25 GiB in history + 12.5 GiB live; [memory.md](memory.md)). | Out-of-memory risk on huge documents; typical documents (10 layers, partial) peak at a few hundred MiB. | Measured in 4G; the rule keeps every step undoable (invariant H4, ADR 0015), and changing it needs an ADR. | Warn before a conversion whose step exceeds the budget, or offer it as non-undoable; never drop it silently. |
| 35 | **Colour state is not persisted**: recent colours and swatches live for the session only; no colour sets from files (D-5A-6). | A user's palette is lost on exit. | No preferences system exists yet; documents must not carry tool state (ADR 0021). | Persist recent colours and swatches with the user-preset store (#8), in a user directory, as versioned JSON. |
| 36 | **No transparent colour** (the reference's third slot and `C`); picking a transparent pixel keeps the colour (D-5A-1, D-5A-2). | Erasing needs the eraser tool or the stylus eraser end. | `C` is "clear layer" in Mirae; the owner decides whether to add it. | A third slot that paints with the eraser's rule; a shortcut map review. |

## Resolved

- Phase 5A: #22 (the app painted black only: the paint colour, the colour
  panel and the eyedropper, ADR 0021).
- Phase 4G: none (no code debt fixed; the PNG limit error and the import
  notice were fixed as audit findings, not listed debt).
- Phase 4F: #27 (16 → 8 rounding vs the reference's import truncation:
  decided, D-4F-3).
- Phase 4E: #2 (8-bit layer storage: 16-bit documents, ADR 0018; 8-bit
  stays the default, D12).
- Phase 4B: #4 (no undo/redo; ADR 0015).
- Phase 4A: #10 (`Document` public fields; a layer API with stable ids,
  ADR 0014).
- Phase 3.5: raw HWND in the Ink API; unit conversion functions named
  after their input; duplicated stroke-override rule in app and UI; magic
  preset id strings; golden updates triggered by any value of the
  variable; effector stream overlap beyond 64 effectors; `format_version`
  overflow; unenforced crate boundaries.
- Moved to the roadmap (an idea, not a defect): #7 movement prediction.
