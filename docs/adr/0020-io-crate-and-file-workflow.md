# 0020 — `tela-io`, its dependencies, and the app's file workflow

Phase 4F. Companion of ADR 0019 (the format).

# Context

Files are untrusted input, need a container library, a PNG codec and
JSON, and the desktop workflow needs native file dialogs and a way to
ask about unsaved changes. The model (`tela-document`) must not learn
about files, paths or dialogs, and the UI must hold no authoritative
state (ADR 0011, invariant B4).

# Decision

1. **A new crate `tela-io`** (layer 2: document, canvas, color, `serde`,
   `serde_json`, `png`, `zip`) owns the `.mirae` format, atomic saving,
   limits, PNG export and import. `document` does not depend on it; it
   offers what I/O needs as ordinary API: `Document::from_parts`
   (validated construction), `next_layer_id`, `LayerPixels::from_surface`,
   `LayerMask::reveal_tile`, `add_layer_with_pixels` (one undo step),
   `paper_backdrop`, `ComposeScratch::premultiplied`, `revision`.
2. **`zip` 9 with `default-features = false, features =
   ["deflate-flate2-zlib-rs"]`**: stored and deflate only, pure-Rust
   zlib-rs (already in the lock file through flate2), no encryption, no
   other codecs, no clock (deterministic timestamps). Its reader collapses
   duplicate names, so the loader checks the end records itself first.
3. **PNG**: the existing `png` crate. Export composes with the document's
   own compositor in `f32` and quantizes once, 8 or 16 bits, RGBA, `sRGB`
   chunk. Import accepts every colour type, premultiplies in `f64` and
   quantizes once to the document's depth; it is centred like the
   reference's import.
4. **Errors and warnings**: `IoError` (filesystem, not a document,
   malformed archive, unsupported version, invalid manifest naming the
   field, invalid document, corrupt tile, limit exceeded, unsupported,
   invalid image, internal) and `Warning` (unknown field or entry, colour
   space ignored, 16 → 8 reduction, first frame only), returned to the
   app, never dropped. Public readers catch panics from decoders as
   `Internal` (defence in depth); tests drive the unguarded paths.
5. **Modified state at the document level**: `Document::revision()`
   (history step serials + a count of non-recorded persistent changes)
   is the save point; the app stores the revision of its last save. Undo
   back to it is clean; visibility counts until saved (reference F3).
6. **The app owns the workflow** (`app/files.rs`): native file dialogs
   through `rfd` (app only), Ctrl+N/O/S, Save As on Ctrl+Shift+S and
   Shift+Alt+S, drag-and-drop, the title marker. Anything that replaces
   or closes a modified document waits in a pending action until the
   unsaved-changes prompt is answered (Save / Don't Save / Cancel).
7. **The prompt and error notices are egui modals** (`tela_ui::prompt`),
   not native message boxes: a `MessageBoxW` opened inside winit's event
   handler is never shown (it waits for an idle queue that the event
   loop keeps refilling), observed in the running app. Native file
   dialogs (`IFileDialog`) work and are kept.
8. **Synchronous I/O, no parallel compression** in this phase: measured
   adequate except for very large 16-bit documents (~6 s); background
   and parallel saving are later work with this serializer.

# Consequences

- Crate graph: `io -> document, canvas, color`; `app -> io, rfd`;
  enforced by `crate_boundaries.rs`.
- Autosave can reuse `write_document` (any `Write + Seek`) unchanged.
- The UI gained the document bar's file commands and two modals, still
  lent data only.
- `rfd` is the only new dependency outside `tela-io` (Windows: the same
  `windows-sys` 0.61 the workspace already uses).

# Alternatives considered

- **I/O inside `document`**: couples the model to serde, zip and png and
  to files; rejected by the boundary rules.
- **A hand-written ZIP reader/writer**: fewer dependencies, more code to
  get right for hostile input; the maintained crate with a narrow
  feature set and our own pre-checks is safer.
- **Native message boxes** (`rfd::MessageDialog`, or a manifest for
  task dialogs): invisible under winit's handler (see 7); a task dialog
  needs comctl32 v6 and an application manifest for no gain over egui.
- **Background-thread dialogs**: an owned message box across threads
  deadlocks the owner; an unowned one leaves the window unresponsive.
- **A plain `modified` flag**: stays modified after undoing to the save
  point and needs every mutation to remember to set it; the revision is
  computed from what the document already records.
