# Phase 4 — layers and document

Phase report, written at the end of 4G (2026-10-16). Plan, decisions
D1–D19 and the subphase rules: [PLAN.md](PLAN.md). Parity, subphase by
subphase: [../PARITY.md](../PARITY.md) and [../parity/](../parity/).

Phase 4 turned a single-canvas brush engine into a raster document editor:
a layer stack with history, blend modes, masks, clipping, alpha lock, two
storage depths and a file format, each behaviour taken from the reference
(Clip Studio Paint, black box) where it defines one, observed before it
was implemented. The brush engine (phases 1–3) did not change.

## What was built

| Subphase | Built | Decision record | Parity record |
|---|---|---|---|
| 4A layers | `Document` with a layer API and stable ids (never reused); create, select, delete, reorder, visibility, opacity, lock; one pixel-mutation path; layer panel | [ADR 0014](../adr/0014-document-layer-model.md) | [4A](../parity/4A-layers.md) |
| 4B undo/redo | Linear, swap-based history inside `Document`; the reference's step semantics; 200 steps / 1 GiB | [ADR 0015](../adr/0015-undo-redo-tile-swap.md) | [4B](../parity/4B-undo-redo.md) |
| 4C blend modes | Normal + 16 modes; compositing space chosen by measurement (sRGB-encoded, not linear light); the paper as backdrop for blend tiles | [ADR 0016](../adr/0016-blend-modes-and-compositing-space.md) | [4C](../parity/4C-blend.md) |
| 4D masks, clipping, alpha lock | One sparse 8-bit mask per layer; isolated clipping groups with source-atop; alpha lock as source-atop that does not stop the eraser | [ADR 0017](../adr/0017-masks-clipping-alpha-lock.md) | [4D](../parity/4D-masks-clipping-alpha-lock.md) |
| 4E 16-bit | Per-document depth, premultiplied sRGB-encoded `u16`; exact 8 → 16, nearest 16 → 8, conversion as one undo step; 8-bit default | [ADR 0018](../adr/0018-16-bit-storage.md) | [4E](../parity/4E-16bit.md) |
| 4F document I/O | `.mirae` v1 (ZIP + JSON manifest + raw tiles), `tela-io`, atomic deterministic save, hostile-input loading with limits, v1 fixtures; PNG export/import; Ctrl+N/O/S, Save As, native dialogs, drag-and-drop, unsaved-changes prompt. The product is named Mirae | [ADR 0019](../adr/0019-mirae-document-format-v1.md), [ADR 0020](../adr/0020-io-crate-and-file-workflow.md), [format](../formats/mirae-v1.md) | [4F](../parity/4F-file-workflow.md) |
| 4G integration | End-to-end tests, `doc_bench`, memory model, audits, a real-app session, documentation, the next-phase proposal; three audit fixes, no new features | — | [4G](../parity/4G-integration.md) |

Tests: 207 at the end of phase 3.5 → 225 (4A) → 255 (4C) → 288 (4D) →
324 (4E) → 386 (4F) → **401** (4G), all passing; clippy, fmt and docs
clean.

## Decisions taken by evidence

- **Compositing space** (4C): W3C formulas on sRGB-encoded values match
  the reference within 1–2/255; linear light is 54–196/255 off.
- **The paper takes part in blending** (4C), as in the reference; Mirae
  has a paper colour, not a paper layer (ADR 0014, D-4A-1).
- **Clipping** (4D): isolated groups with source-atop where the blend
  result replaces the colour; W3C atop was 50/255 off, non-isolated up to
  215/255.
- **Masks** reveal by stroke alpha whatever the colour; erasing hides; the
  eraser ignores alpha lock (4D, observed).
- **8-bit default** (4E, D12): the reference's only depth, half the
  memory; 8 → 16 is lossless at any time.
- **Undo semantics** (4B) follow the reference: visibility and selection
  are not steps; consecutive opacity edits merge (confirmed by the owner).
- **16-bit PNG import rounds** where the reference truncates (4F,
  deliberate); **undo back to the saved state is clean** (deliberate).

## Main numbers

Ryzen 7 5800X, Windows 11, release build; details in
[../performance.md](../performance.md) and [../memory.md](../memory.md).

| What | Number |
|---|---|
| Stamping | ~375 000 dabs/s at 5 px, ~480 at 500 px; 0.000 allocations per dab; ~3× parallel at 500 px |
| Composition, 2 layers + stroke | 0.107–0.13 ms per tile (unchanged through phase 4 within noise) |
| Composition per tile, every layer drawing | 10 / 50 / 100 Normal layers: 1.6 / 6.4 / 12.4 ms; 100 Multiply 29.7 ms; 100 Soft Light 44.4 ms; 16-bit +0–10 %; 0 allocations |
| Stroke frame (4 tiles), 4096² documents | 100 sparse layers 5.3 ms; 50 partial layers 11.3 ms; 10 dense layers 8.8 ms (8-bit) |
| History | undo/redo ≤ 0.08 ms; commit 0.6–0.95 ms per touched tile (4096², `doc_bench`); 256 KiB (8-bit) / 512 KiB (16-bit) per changed tile per step |
| 16-bit vs 8-bit | composition 0.99–1.07×, commit 1.2–1.5×, memory 2× |
| Banding (repeated faint strokes) | 16-bit within 0.75 level of exact; 8-bit 25–136 levels off |
| Files, 4096² × 10 full | 8-bit save 2.1 s / load 0.8 s; 16-bit 6.4 / 1.7 s; 1.3–3 MiB extra memory |
| Files, 100 sparse layers | 8-bit save 2.1 s / load 0.65 s (122 MiB file); 16-bit 8.4 / 1.8 s |

## Invariants added

Document L1–L10, history H1–H12, blend modes C1–C7, masks M1–M5, alpha lock A1–A3, clipping CL1–CL6, storage depth
DP1–DP7, files IO1–IO10, integration I1–I5 (where they are checked
together), and F3 now also guarded by `doc_bench`
([../architecture/invariants.md](../architecture/invariants.md)). Every
invariant names its tests; B4 (the UI holds no authoritative state) is
the only one protected by review alone.

## Parity at the end of the phase

Summary in [../PARITY.md](../PARITY.md#phase-4-parity-summary). Validated
within recorded scopes: layer commands during a stroke, undo/redo
semantics, all 17 blend modes at 100 % and 60 % over the paper (3 400
combinations), masks, clipping and alpha lock (2 × 2 176 swatches), the
8-bit default and exact widening, the unsaved-changes prompt, PNG export
(pixel-equal with the paper) and import. Known differences are listed per
subphase and reviewed in [4G](../parity/4G-integration.md). Not measured:
anything needing a physical tablet, blending over a translucent backdrop
without paper (D-4C-1), performance against the reference, colour-dependent
checks in the running app (it paints black only), phases 1–3.

## Limitations at the end of the phase

- No physical tablet validated (synthetic Windows Ink only).
- The app paints black only; no selections, transforms, fill, gradients,
  folders, merge down, vector layers or text.
- CPU composition is single-threaded and linear in layers (#33); saving
  and loading block the UI thread (#28); no autosave or recent files
  (#29).
- Blending over a translucent backdrop without paper is unmodelled
  (D-4C-1); transparent export refuses the cases it affects.
- Display texture limited by the GPU's maximum size (#3); 8-bit display.
- A depth conversion of a huge document holds both copies (#34).

## Remaining debt

[../TECHNICAL_DEBT.md](../TECHNICAL_DEBT.md), regrouped at the end of 4G:
P1 — #1 physical tablet, #20 translucent backdrop (prerequisite of merge
down and folders). P2 — #3, #8, #17, #18, #21, #22, #23, #28, #29, #32,
#33. P3 — the rest. Resolved during phase 4: #2, #4, #10, #27.

## Next

The next macrophase is **proposed, not started**:
[../phase5/PROPOSAL.md](../phase5/PROPOSAL.md).
