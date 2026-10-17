# Architecture decision records

Each ADR records one decision that is already in the code: the context,
the decision, its consequences and the alternatives considered. ADRs are
not rewritten when circumstances change; a new ADR supersedes an old one
and both say so.

| # | Decision | Phase |
|---|---|---|
| [0001](0001-premultiplied-alpha.md) | Premultiplied alpha for all compositing | 1 |
| [0002](0002-sparse-256-tiles.md) | Sparse 256×256 tiles | 1 |
| [0003](0003-separate-stroke-buffer.md) | Separate stroke buffer; preview and commit share one path | 1–3 |
| [0004](0004-cpu-composition-gpu-display.md) | CPU composition of dirty tiles, GPU display texture with mips | 1, 3 |
| [0005](0005-one-euro-default-stabilizer.md) | One Euro filter as the default stabilizer | 2 |
| [0006](0006-centripetal-catmull-rom.md) | Centripetal Catmull–Rom as the default interpolation | 2 |
| [0007](0007-deterministic-stroke-rng.md) | Deterministic, counter-based stroke randomness | 3 |
| [0008](0008-versioned-json-presets.md) | Versioned JSON brush presets (+ phase 3.5 review) | 3 |
| [0009](0009-canvas-space-textures.md) | Paper textures in canvas space, modulating the opacity ceiling | 3 |
| [0010](0010-bit-identical-parallel-stamping.md) | Bit-identical per-tile parallel stamping | 3 |
| [0011](0011-egui-isolated-in-ui.md) | egui isolated in `tela-ui` | 3 |
| [0012](0012-golden-visual-corpus.md) | Golden visual corpus for brushes | 3 |
| [0013](0013-airbrush-time-instants.md) | Airbrush deposition at fixed time instants | 3 |
| [0014](0014-document-layer-model.md) | Document layer model: stable ids, relative placement, one pixel-mutation path | 4A |
| [0015](0015-undo-redo-tile-swap.md) | Undo/redo inside `Document`, swap-based steps, reference-derived semantics, byte/step limits | 4B |
| [0016](0016-blend-modes-and-compositing-space.md) | Blend modes: W3C formulas on sRGB-encoded values (chosen by measurement), paper as backdrop for blend tiles | 4C |
| [0017](0017-masks-clipping-alpha-lock.md) | Layer masks (inverted sparse 8-bit), isolated clipping groups with opaque-blend source-atop, alpha lock as source-atop (all measured) | 4D |
| [0018](0018-16-bit-storage.md) | 16-bit storage: per-document depth, premultiplied sRGB-encoded `u16`, exact 8 → 16, nearest 16 → 8, 8-bit masks and display, 8-bit default (measured) | 4E |
| [0019](0019-mirae-document-format-v1.md) | `.mirae` v1: ZIP + JSON manifest + raw premultiplied tiles, explicit sparsity, exact numbers, strict versioning, validated loading, limits, atomic save, no history, fixtures as contract | 4F |
| [0020](0020-io-crate-and-file-workflow.md) | `tela-io` crate and dependencies (zip deflate-only, png), errors/warnings, revision-based save point, app-owned workflow with egui prompts and native file dialogs | 4F |
| [0021](0021-paint-colour-and-colour-tool-state.md) | The paint colour: opaque straight `f32` `Color` without alpha, captured by the stroke, `(C·m, m)` formula; colour state is session tool state in the app (not the document, not history); eyedropper on the composite or the layer | 5A |
| [0022](0022-blending-over-translucent-backdrops.md) | Blending over a translucent backdrop (D-4C-1 resolved by measurement): Multiply W3C, the other modes compose the blended colour over the backdrop first, additive alpha for Screen/Dodge/Hard/Soft Light; opaque backdrops unchanged | 5B |
| [0023](0023-merge-down-and-layer-workflow.md) | Merge down composes the pair alone (lower layer keeps id, name, mode, clip; opacity baked; masks applied or kept as asked), one swap-based step holding only changed tiles; consecutive renames are one step; drag reorder through relative placement | 5B |
| [0024](0024-selections.md) | Selections: document state, 8-bit coverage stored sparse as coverage XOR fill (select all/invert store nothing), empty = none, measured operators (saturating sum/difference, truncated product) and feather (Gaussian σ = 0.3·px, renormalized over the canvas), strokes scaled by coverage (m·s) in the shared preview/commit rule, swap-based history steps, not saved | 5C |

## Writing a new ADR

Copy the structure (`# Context`, `# Decision`, `# Consequences`,
`# Alternatives considered`), number it sequentially, add it to this
table, and reference it from the code or docs it affects. Write one when a
change alters an invariant, a crate boundary, a file format, a default
chosen by benchmark, or anything a future contributor would otherwise
"fix" back.
