# Reference measurements: fill (phase 5E)

The reference application (CLIP STUDIO PAINT EX 5.1.5) filling **our own**
test documents (`cargo run -p tela-corpus --example fill_probe -- psd
<kind> <path>`, see `crates/corpus/examples/fill_probe.rs`): 400 × 300
canvases with an opaque black layer, an opaque white layer above it and a
layer "ink" that the fill tool ("Preencher", sub-tool "Referir apenas à
camada de edição") edits. Fill colour: magenta (255, 0, 255). Procedure
and observations: docs/parity/5E-fill.md.

## Inputs

`conn.png`, `tol.png`, `alpha.png`, `lock.png`: the ink layer of each
probe document, **straight** RGBA as written into the PSD
(`fill_probe png <kind>`). `alpha.png` stores colours under alpha 0
(hidden colours); Mirae's premultiplied storage drops them, which is the
point of that probe.

## Results

`islands.csv`: one line per fill (the over-white capture classified by
`fill_probe corpus`): `kind` (`tol` or `alpha`), `zone` (alpha only: 0
transparent, 1 opaque black, 2 opaque white seed), `tenths` (tolerance in
tenths of a percent), the seed pixel, `chain_last` (`tol` only: the last
stripe of the one-level chain reached; stripe k has grey 128 + k) and
`islands`, a hex string of bits (bit k of byte k / 8 = island k of
`fill_probe`'s island list reached by the fill). Anti-aliasing off.

The PNGs below are **decoded** (two captures at 100 % zoom, over the white
layer and over the black one, `fill_probe decode`): the ink layer's
premultiplied RGBA stored as the PNG's bytes, not straight-alpha PNGs.
Exception: `*_white.png` are plain captures over white.

| File | Document, settings, seed |
|---|---|
| `tol_aa_100`, `tol_aa_300` | `tol`, tolerance 10 % / 30 %, anti-aliasing **on**, seed (2, 2) |
| `ring_aa`, `ring_noaa` | `conn`, tolerance 10 %, anti-aliasing on / off, seed (240, 150) inside the anti-aliased ring |
| `feather10` | `conn`, rectangle selection (170, 205)–(290, 265) feathered 10 px ("Desfocar borda"), tolerance 10 %, AA off, seed (230, 235) |
| `alock_1000`, `alock_100` | `lock`, the ink layer's alpha lock on, tolerance 100 % / 10 %, AA off, seed (10, 10) |
| `lock_1000` | `lock`, alpha lock off, tolerance 100 %, AA off, seed (10, 10) |
| `fill_all`, `fill_all_alock` | `lock`, Edit › Preencher (Alt+Del) without a selection; without / with alpha lock |
| `diamond_white`, `checker_white`, `corridor_white` | `conn`, tolerance 10 %, AA on, close gap off, seeds (150, 50), (18, 140), (225, 50) |

Nothing here comes from the reference application's files: these are its
observed results on our inputs.
