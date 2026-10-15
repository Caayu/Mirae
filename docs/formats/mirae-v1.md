# The `.mirae` document format, version 1

Mirae's own, open document format (ADR 0019). It stores everything
needed to reconstruct a document exactly — every layer, property, mask
and stored pixel — and nothing that can be derived (composites, caches,
thumbnails) or that belongs to the session (undo history, view, tools).
This page is the normative description; `crates/io` implements it and
`crates/io/tests/fixtures/v1/` holds the compatibility fixtures every
future reader must load (see "Compatibility").

## Container

A ZIP archive (PKWARE APPNOTE; ZIP64 when sizes require it). Entries are
**stored** or **deflate**-compressed (methods 0 and 8) and never
encrypted. Writers produce entries in this order; readers do not depend
on the order:

```
mimetype                           stored, first
manifest.json                      deflate
layers/<id>/pixels/<x>_<y>         one pixel tile per entry
layers/<id>/mask/<x>_<y>           one mask tile per entry
```

- `mimetype` holds exactly the ASCII bytes `application/x-mirae` (no
  newline). Being first and stored, its name is at byte 30 of the file
  and its content at byte 38, so the file can be identified by its first
  57 bytes.
- `<id>` is the layer's id in decimal; `<x>_<y>` the tile's coordinates
  in decimal, possibly negative (`layers/7/pixels/-1_3`).
- Entry names are relative, `/`-separated, at most 1024 bytes, with no
  empty, `.` or `..` components, backslashes, colons or control
  characters. Directory entries (names ending in `/`) carry no data.
- The writer stores a fixed modification time (1980-01-01) in every
  entry, so the same document always gives the same bytes.

## Manifest (`manifest.json`)

A UTF-8 JSON object. Writers emit the fields in the order below.

| Field | Type | Meaning |
|---|---|---|
| `format` | string | Always `"mirae"`. |
| `version` | integer ≥ 1 | The format version; this page describes `1`. |
| `writer` | string | Informational (`"Mirae 0.1.0"`); optional, never interpreted. |
| `canvas` | object | `width`, `height`: canvas size in pixels (integers ≥ 1). |
| `depth` | string | `"rgba8"` or `"rgba16"`: bits per channel of every layer's pixels. |
| `paper` | array of 4 numbers | Paper colour, straight RGBA in `0..=1`, sRGB-encoded; alpha 1. |
| `next_layer_id` | integer or `null` | The id the next new layer gets (greater than every id ever used); `null` once 4 294 967 295 has been used. Required. |
| `active_layer` | integer | Id of the selected layer. |
| `active_part` | string | `"pixels"` or `"mask"` (only if the active layer has a mask): what strokes edit. |
| `layers` | array | The layers, **bottom to top**, at least one. |

Each layer:

| Field | Type | Meaning |
|---|---|---|
| `id` | integer 1 … 4 294 967 295 | Unique in the document; names its tile entries. |
| `name` | string | Display name (any UTF-8). |
| `visible` | bool | Hidden layers are not composed. |
| `opacity` | number in `0..=1` | Layer opacity. |
| `blend` | string | One of `normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`, `add` (ADR 0016). |
| `locked` | bool | Refuses pixel, opacity, blend and removal edits. |
| `alpha_locked` | bool | "Lock transparent pixels" (ADR 0017). |
| `clipped` | bool | Clipped to the layer below (clipping group, ADR 0017). |
| `pixels` | array of `[x, y]` | The pixel tiles stored for this layer, each once. |
| `mask` | object or `null` | `enabled` (bool) and `tiles` (array of `[x, y]`): the layer mask, or no mask. |

Numbers: integers are JSON integers. `opacity` and the paper channels
are 32-bit floats written as the decimal of their exact value (through
`f64`): reading the number as a double and rounding it to a float gives
the same bits back. Tile coordinates are within ±8 388 607 (so every
pixel has a 32-bit coordinate).

## Tiles

A tile is 256 × 256 pixels; tile `(x, y)` covers canvas pixels
`[256x, 256x + 256) × [256y, 256y + 256)`. Pixels are row-major, top row
first, left to right. A tile that is not listed **does not exist**:
its pixels are transparent (pixel tiles) or fully revealing (mask
tiles). Listed tiles may lie outside the canvas (layers keep pixels
painted beyond its edges).

| Entry | Size | Content |
|---|---:|---|
| pixels, `rgba8` | 262 144 bytes | 4 bytes per pixel: R, G, B, A |
| pixels, `rgba16` | 524 288 bytes | 8 bytes per pixel: R, G, B, A as 16-bit **little-endian** |
| mask | 65 536 bytes | 1 byte per pixel |

Pixel values are **premultiplied** by alpha and **sRGB-encoded** — exactly
the values Mirae stores in memory (ADR 0001, 0016, 0018), so saving and
loading are bit-exact. Every byte pattern is a valid stored value.

Mask bytes are **reveal** values: 255 shows the layer, 0 hides it. A
listed mask tile must hide something (not all 255): a mask that shows
everything there simply has no tile. (In memory Mirae stores the hidden
amount, `255 − reveal`; the file stores the intuitive value.)

There is no per-tile header: the manifest's `depth` and the entry path
determine the size, and each entry's ZIP CRC-32 protects its content.

## Reading rules

A reader must, before using the data:

1. Check the ZIP end records (entry count, directory size) against its
   limits, refuse duplicate entry names and unsafe names.
2. Require `mimetype` to be exactly `application/x-mirae`; otherwise the
   file is not a Mirae document.
3. Read `manifest.json` (bounded size); check `format`, then `version`
   **before any other field** (see "Versioning").
4. Check every field's type and range; refuse unknown `depth` or
   `blend` values (an error, never a fallback: a substitute would change
   how the document looks).
5. Require every listed tile to exist with exactly the size above and a
   valid CRC; refuse any entry under `layers/` that is not listed (data
   that would otherwise silently vanish). Decompress no entry beyond its
   expected size.
6. Build the document only if it is consistent: ids unique and non-zero,
   `next_layer_id` greater than every id, `active_layer` existing, a mask
   target only with a mask, opacity in `0..=1`, opaque paper, no mask tile
   that reveals everything (Mirae: `Document::from_parts`).

Unknown fields in a known version and unknown entries outside `layers/`
are ignored and reported as warnings. They are **not** kept when the
document is saved again.

## Limits (Mirae's defaults)

Checked before the data they govern is read; saving refuses a document
beyond them, so Mirae never writes a file it would refuse to read. An app
may lower them.

| Limit | Default |
|---|---:|
| Canvas width or height | 65 536 px |
| Layers | 10 000 |
| Stored tiles (pixels + masks) | 1 000 000 |
| Decoded pixel and mask data | 16 GiB |
| Layer name | 4 096 bytes |
| Manifest | 16 MiB |
| ZIP entries | 1 001 024 |
| ZIP central directory | 256 MiB |

## Versioning and compatibility

- `version` is a single integer, incremented whenever a reader of the
  previous version could not load a new file **faithfully** (a new field
  that changes appearance, a new blend mode or depth, a new tile
  encoding). Additions that an older reader may safely ignore (purely
  informational fields, ignorable entries) keep the version.
- A reader loads every version up to its own, migrating older ones step
  by step. A **newer** version is refused as a whole (`UnsupportedVersion`):
  nothing is loaded rather than part of a document. Version 0 does not
  exist.
- Every valid version-1 file must open in every later Mirae. The fixtures
  in `crates/io/tests/fixtures/v1/` are the contract: they are never
  regenerated, and `tests/fixture.rs` pins their content (structure,
  sample values, a content digest, the files' own bytes).

## What is not stored

Undo history (ADR 0015; a loaded document starts with an empty history),
the view (pan, zoom, rotation), tools and brush presets (ADR 0008),
composites, thumbnails, colour profiles (Mirae has no colour management
yet; values are sRGB-encoded), resolution (Mirae documents have none).

## Saving

Writers replace a file atomically: the new archive is written to a
temporary file in the same directory, flushed and synced, then renamed
over the target; a failure leaves the previous file untouched.
