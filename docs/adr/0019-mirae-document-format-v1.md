# 0019 — The `.mirae` document format, version 1

Phase 4F. Specification: docs/formats/mirae-v1.md. Written after the
reference's file workflow was observed (docs/parity/4F-file-workflow.md)
and the format measured (`io_bench`, docs/performance.md).

# Context

Mirae had no document files. Once users have files, the format is a
compatibility promise that is expensive to break (phase-4 risk 3). The
document model is settled (ADR 0014–0018): a stack of raster layers with
stable ids and properties, sparse 256-px tiles of premultiplied
sRGB-encoded pixels at 8 or 16 bits, 8-bit masks stored inverted, clipping
flags, alpha lock, a paper colour, the active layer and edit target, a
never-reused id counter, and an undo history. The reference's own format
is not inspected (clean room); `.mirae` is Mirae's identity, not a copy.

# Decision

1. **Container: ZIP**, entries stored or deflated only. It is public,
   streamable per entry, random-access through its central directory,
   CRC-protected per entry, inspectable with ordinary tools, and ZIP64
   lifts the 4 GiB limit. The `mimetype` entry comes first and stored
   (as in OpenDocument and OpenRaster) and holds `application/x-mirae`.
2. **Layout**: `manifest.json` plus one entry per tile,
   `layers/<id>/pixels/<x>_<y>` and `layers/<id>/mask/<x>_<y>`. Nothing
   else in v1 (no placeholders for future data).
3. **Manifest**: JSON with exactly the state needed for reconstruction:
   format, version, canvas, depth, paper, next layer id, active layer,
   active part, and per layer (bottom to top) id, name, visible, opacity,
   blend name, locked, alpha lock, clipped, the list of its pixel tiles,
   and its mask (enabled, list of tiles) or null. Nothing derivable
   (composites, caches, thumbnails) and no session state (undo history,
   view, tools). The active layer and part are kept: they are cheap and
   reopen work where it was left; they are not modifications (reference
   F2). The persisted model is a set of DTOs in `tela-io`, separate from
   the domain types.
4. **Tiles: raw**, no header, no per-tile compression: 256 × 256 pixels,
   R G B A premultiplied sRGB-encoded as stored in memory (bytes at 8
   bits, little-endian `u16` at 16 bits); masks as 8-bit **reveal**
   values (255 shows). Raw is the simplest exact encoding at both depths
   (PNG tiles would need premultiplication round trips or 16-bit
   straight-alpha conversion and would not be bit-exact for
   premultiplied data); the size is fixed by the depth, so a wrong size
   is detectable; ZIP's deflate compresses (default level 2, measured).
   The mask is stored as the user sees it, not in its inverted in-memory
   form: the file documents the concept, not an optimization; a stored
   tile must hide something, so the mask's sparse layout is canonical.
5. **Sparsity is explicit**: the manifest lists every stored tile; a tile
   not listed does not exist. Absent tiles stay absent, present blank
   tiles stay present, negative and off-canvas coordinates are kept
   (D18). The lists also make missing or extra entries detectable.
6. **Exact numbers**: opacity and paper channels are `f32` written as the
   decimal of their exact value (via `f64`), so reading as a double and
   rounding gives the same bits even with an imprecise JSON parser.
7. **Versioning**: one integer `version`, `1`. It changes when an older
   reader could not load a file faithfully. Readers load their version
   and older ones (migrations), refuse newer ones **as a whole**, and
   check the version before any other field. Unknown fields of a known
   version and unknown entries outside `layers/` are ignored and
   reported as warnings (not preserved on re-save). Unknown enum values
   (blend mode, depth) are errors, never mapped to a default: integrity
   of appearance first. (This tightens the plan's "lenient unknown
   values", which suits brush presets — ADR 0008 — but not documents.)
8. **Validation**: the loader constructs documents only through
   `Document::from_parts`, which checks every structural invariant; the
   I/O crate cannot build an invalid document. Identity checks needed to
   name entries (zero or duplicate ids) are also done on the manifest.
9. **Untrusted input**: explicit, documented `Limits` (canvas side,
   layers, tiles, decoded bytes, name and manifest size, ZIP entries and
   directory size) are checked before the data they govern is read; ZIP
   end records are checked before the archive is opened; duplicate and
   unsafe entry names are refused; no entry is decompressed past its
   expected size; checksums are enforced. Saving checks the same limits.
10. **Atomic save**: temporary file in the target's directory, flush,
    `fsync`, rename over the target; failures remove the temporary file
    and leave the old one intact. Output is deterministic (fixed entry
    times, sorted tiles).
11. **History is not stored** (v1): it is large (whole tiles per step),
    tied to the session's memory budget, and no observed workflow needs
    it; a loaded document starts with an empty history and clean.
12. **Compatibility contract**: two committed v1 fixtures (8- and 16-bit,
    every property) must load forever as `tests/fixture.rs` pins them;
    they are never regenerated.

# Consequences

- Save → load is bit-exact at both depths for everything the format
  stores (tests, property tests over random documents, the fixtures);
  save ∘ load ∘ save reproduces the same bytes.
- Measured (4096² × 10 full layers): 8-bit 640 MiB of pixels → 84 MiB in
  1.9 s, loaded in 0.7 s; 16-bit 1.25 GiB → 315 MiB in 6.1 s, loaded in
  1.6 s; peak extra memory 2–3 MiB for any size (one tile at a time).
  Large 16-bit saves are slow enough to want background saving later.
- A file can be read with any ZIP tool and a JSON viewer; the tile data
  needs only the table in the specification.
- Unknown fields from newer compatible writers are dropped on re-save
  (warned at load).
- Formats of the reference (and PSD) stay out of scope; interchange is
  PNG.

# Alternatives considered

- **A single binary file** (custom chunks): tighter, but needs its own
  integrity, random access and tooling; ZIP has all three.
- **SQLite**: transactional and random-access, but a large dependency,
  non-trivial to validate as hostile input, and opaque to simple tools.
- **PNG (or other image) tiles inside the ZIP**: lossy for premultiplied
  data unless re-premultiplied exactly, awkward at 16 bits, slower; raw
  is exact and the container compresses.
- **Tile set implied by the entries** (no lists in the manifest): smaller
  manifest, but missing or stray entries would go unnoticed.
- **Serializing the domain types directly** (`derive(Serialize)` on
  `Document`): couples the file to internals and makes migrations and
  validation harder; rejected by the brief and here.
- **Storing the inverted mask as in memory**: one inversion less, but the
  file would expose an optimization; reveal values are the concept.
- **Lenient loading of newer versions** (as presets do): could show a
  document missing what made it look as it did; refused.
- **Storing undo history**: large, session-specific, no need observed.
- **Per-tile zstd or uniform-tile shortcuts**: possible in a later
  version if measurements ask for it.
