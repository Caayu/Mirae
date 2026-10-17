# Parity record 4F — file workflow and PNG fidelity

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440. Input: mouse via SetCursorPos/mouse_event, keyboard via
keybd_event. Observed 2026-10-15, **before** any 4F code was written.
Black-box only: the reference's UI was operated as a user; results were
read from its title bar, tabs, dialogs, the files it wrote (PNG headers
and pixels; its own `.clip` files only by size and modification time,
never opened or parsed) and the Windows event log. It opened only test
files **we wrote**. Nothing was read from its install or user-data files
and no preference was changed.

The `.mirae` document format is Mirae's own; it has no counterpart to
match. What is compared is the **workflow** (save, open, unsaved
changes) and **PNG fidelity** (export, import, 16-bit images).

## Part 1 — the reference (observed)

### Method

1. **Workflow.** Ctrl+N (default dialog: 1600 × 1200, white paper),
   then selecting a layer, hiding and showing a layer, Ctrl+S, a stroke
   with the pen, Ctrl+Z / Ctrl+Y, Ctrl+W (each button of its prompt),
   Ctrl+O of the saved file, Ctrl+Q with unsaved documents (cancelled),
   and the Arquivo menu. The title bar and tabs were captured after each
   step.
2. **PNG export.** Arquivo › Exportar (camada única) › .png, default
   settings, with the paper layer hidden ("transparent") and shown. The
   files' chunks were listed (`IHDR`, ancillary chunks) and their pixels
   sampled.
3. **PNG import.** `cargo run --release -p tela-corpus --example
   png_probe -- write <dir>` writes three test images of our own, each a
   swatch grid in a 2 px opaque magenta frame:
   - `rgba8.png`: RGBA 8-bit, straight alpha, 10 colours × alphas 255,
     254, 200, 128, 100, 64, 17, 5, 1, 0;
   - `rgb8.png`: the same colours, RGB 8-bit;
   - `rgba16.png`: RGBA 16-bit; 40 opaque greys `i·1677 + 129` (low
     byte set, so truncating and rounding to 8 bits differ) and white
     with 16-bit alphas (65535 … 300).
   Each was imported (Arquivo › Importar › Imagem) into a new default
   document, the paper hidden, and exported to PNG.
   `png_probe read <kind> <export.png>` finds the frame and compares each
   swatch with the models it prints (data in [4F/](4F/)).
4. **Corrupt input.** Files of our own: `bad.clip` (5 000 random bytes)
   and `bad.png` (the PNG signature followed by 1 000 random bytes),
   opened with Ctrl+O.

### Observations

| # | Observation |
|---|---|
| F1 | **A new document is clean.** It is named in the new-document dialog ("Ilustração 20261015 2"); the title shows the name without `*` and its tab a close button `×`. |
| F2 | **Selecting a layer is not a modification** (no `*`). |
| F3 | **Hiding a layer is a modification** (`*` appears) although visibility is not an undo step (4B U9). Showing it again keeps the `*`. |
| F4 | **No save point.** After saving, a stroke adds `*`; undoing it (the canvas is white again), redoing and undoing again all keep the `*`. |
| F5 | **Ctrl+S on a never-saved document** opens the native Windows "Salvar" dialog: name prefilled with the document's name, type "FORMATO CLIP STUDIO", the last folder used. After saving, the title is the file name without extension and without `*`. |
| F6 | **Ctrl+S on a saved document** (and "Salvar" in the close prompt) writes to its file without a dialog (the file's size and time changed). |
| F7 | **Closing a modified document** (Ctrl+W) asks "probe_a foi alterado. Você deseja salvar?" with **Salvar** (default), **Não salvar**, **Cancelar**. Cancelar: the document stays, content intact. Salvar: saved in place (F6), then closed. Não salvar: closed, the file unchanged. |
| F8 | **Open** (Ctrl+O): the native "Abrir" dialog, last folder, filter "Todos os formatos suportados" (PNG files are listed too and open as documents). The file opens in a **new tab**; the reopened `.clip` shows the saved stroke. Other open documents, modified ones included, are untouched and nothing is asked. |
| F9 | **New** with modified documents open: a new tab, nothing is asked (the reference is multi-document). |
| F10 | **Quit** (Ctrl+Q) with six unsaved documents: one prompt "Há 6 telas de pintura que não foram salvas. Você deseja confirmar estas mudanças antes de sair?" with **Confirmar alterações…**, **Descartar alterações**, **Cancelar**. Cancelar keeps everything open. |
| F11 | **Shortcuts** (Arquivo menu): Novo Ctrl+N, Abrir Ctrl+O, Fechar Ctrl+W, Salvar Ctrl+S, **Salvar como Shift+Alt+S**, Sair Ctrl+Q; also Recentes, Salvar cópia, Reverter, Exportar (camada única) › bmp, jpg, png, tif, tga, webp, psb, psd; Importar › Imagem. |
| F12 | **PNG export settings**: colour "Detecção automática da profundidade de cor apropriada", **Incorporar perfil ICC** (on, sRGB IEC61966-2.1), **Exportar transparência** (on), output size (100 %), then a preview dialog. |
| F13 | **Transparent export** (paper hidden): RGBA, 8 bits (colour type 6), chunks `iCCP` ("sRGB IEC61966-2.1"), `pHYs` (2834 px/m = 72 dpi), `tEXt`. |
| F14 | **Export with the paper shown**: still RGBA 8-bit, alpha 255 everywhere; every colour equals the straight colour composed over white, rounded (all 100 swatches, error 0). |
| F15 | **Import** creates a new layer above the active one, named after the file without extension ("rgba8"), as an *image material* layer, **centred** on the canvas (at `((W − w)/2, (H − h)/2)` rounded down: 678, 478 for 244 px in 1600 × 1200), and selects the Object tool with transform handles. At 100 % nothing is resampled. |
| F16 | **RGBA8 import keeps straight colour exactly.** Exported transparent, all 100 swatches equal the input's straight RGBA, including colour at alpha 1 and 5. A premultiplied 8-bit store would miss by up to 127 levels at alpha 1. |
| F17 | **RGB8 import** is exact (alpha 255). |
| F18 | **RGBA16 import truncates to 8 bits** (`v >> 8`): all 80 swatches equal truncation, 22 differ from rounding by one level (alpha too). Same as opening a 16-bit PNG (4E, E5). |
| F19 | **A corrupt `.clip`** gives "Formato de arquivo não suportado." with OK; the application continues. |
| F20 | **A corrupt PNG crashes the reference.** About 2 s after the open, the process ended; the Windows event log has Application Error 1000 "CLIPStudioPaint.exe 5.1.5.0 … ucrtbase.dll … exception code 0xc0000409". The six unsaved documents that were open were lost. |

Not observed: drag-and-drop onto the window (no reliable black-box drag
source from the script), closing a clean document, what `.clip` keeps of
masks and clipping (only a reopened stroke layer was seen; the format
itself is not inspected), PSD details beyond the export entries (F11).
16-bit export: none (4E, E3).

### What this means for Mirae

- Mirae edits **one** document per window; the reference opens tabs.
  Replacing the document (New, Open, a dropped file) must therefore ask
  what closing asks in the reference (F7): **Save / Don't save /
  Cancel**, Save being the default; quitting asks the same once (F10).
  A clean document is replaced without asking.
- Modified state follows F2–F3: visibility counts, selection does not.
  F4 (undo back to the saved state stays modified) is a weakness, not a
  rule to copy: Mirae's history can identify the saved state, so undoing
  back to it is clean again (a deliberate difference).
- Shortcuts: Ctrl+N, Ctrl+O, Ctrl+S as in the reference; Save As on
  **Ctrl+Shift+S** (the owner's request) and **Shift+Alt+S** (F11).
- Native dialogs (F5, F8); the title shows the name and `*` (F1, F5).
- PNG export: 8-bit RGBA, opaque with the paper or transparent (F12–F14);
  the paper case is "straight over white" as in F14.
- PNG import: a new layer above the active one, named after the file,
  centred (F15). Mirae stores premultiplied pixels (ADR 0001), so it
  cannot keep F16's exact colour at very low alpha in 8-bit documents;
  how far it gets is measured below. 16-bit PNGs are rounded or kept
  instead of truncated (F18), deliberately.
- No malformed file may crash Mirae or lose the open document (F19–F20).

## Part 2 — Mirae

### Design derived from the observations (ADR 0019, 0020)

| Obs. | Mirae 4F |
|---|---|
| F1 | A new document is clean; the title is `Mirae — <name>`, with `*` while modified ("Untitled" until saved). |
| F2, F3 | Modified means `Document::revision() != saved`: steps, undo, redo and visibility changes count; selecting a layer or the edit target does not. Showing a hidden layer again stays modified, as in the reference. |
| F4 | Deliberately different: undoing (or redoing) back to the saved state is unmodified again (the revision returns). |
| F5, F6 | Ctrl+S saves in place, or asks with the native save dialog (name prefilled, `.mirae` filter, the last folder) when the document has no file yet. The title shows the file name without extension. |
| F7, F10 | New, Open, a dropped `.mirae` and closing the window ask, when the document is modified, **Save** (default; Enter) / **Don't Save** / **Cancel** (Escape). A save that fails or is cancelled stops the action. Mirae has one document, so quitting asks once. |
| F8, F9 | Ctrl+O opens with the native dialog (`.mirae` filter, last folder). The document is replaced only after the file has fully loaded and validated; a failed open keeps the current document. (One document per window: D-4F-1.) |
| F11 | Ctrl+N, Ctrl+O, Ctrl+S as in the reference; Save As on Ctrl+Shift+S (owner) and Shift+Alt+S (reference). |
| F12–F14 | Export PNG: 8-bit RGBA by default, the composite over the paper (opaque, alpha 255) or transparent; 16-bit on request; an `sRGB` chunk. |
| F15 | Import PNG (button or a dropped `.png`): a new layer above the active one, named after the file, centred, rounded down; one undo step. |
| F16–F18 | Any PNG colour type; premultiplied once at the document's depth: 16 bits kept in 16-bit documents, the nearest level in 8-bit ones (the reference truncates, D-4F-3). |
| F19, F20 | Every malformed file gives an error notice naming the problem (`IoError`), never a crash; the open document stays. |

### Mirae results

Mirae at commit 5645f7b, Windows 11, 2560×1440.

#### Measured (tests)

| Check | Result |
|---|---|
| PNG export with the paper vs the reference's export of the same image (`png_parity.rs`, 1600 × 1200, every pixel) | **Equal** at both depths (0 pixels differ). |
| Opaque RGB8 import, transparent export vs the reference | **Equal** at both depths. |
| Translucent RGBA8 import, transparent export vs the reference | **Equal in 16-bit documents.** In 8-bit documents alpha and opaque pixels are equal; colour at low alpha moves (premultiplied 8-bit storage, up to 127 levels at alpha 1; D-4F-4). |
| RGBA16 import vs the reference | One level off in exactly the 22 of 80 swatches (24 × 24 px each) where rounding and truncation disagree, at both depths (D-4F-3); equal elsewhere. |
| Fully transparent pixels | Mirae writes (0, 0, 0, 0), the reference (255, 255, 255, 0) (D-4F-6; invisible). |
| `png_probe read` on exports from the running app (`4F/tela_read_*.txt`) | 8-bit document, RGBA8: premultiplied-8-bit model exact (0), straight colour off by ≤ 127. 16-bit document, RGBA8: straight colour exact (0). RGBA16 at both depths: nearest level exact (0), truncation off by 1. Placement centred (1878, 1378 for 244 px in 4000 × 3000). |
| Save → load, corruption, limits, fixtures | IO1–IO8 (docs/architecture/invariants.md). |

#### Running app (mouse; a real OLE drag-and-drop from a test window; synthetic Windows Ink pen)

| Check | Result |
|---|---|
| New document, stroke, undo, redo | Title `Untitled` → `Untitled*` → `Untitled` → `Untitled*`. |
| Ctrl+S (no file yet) | Native "Save document" dialog, name "Untitled", type "Mirae document"; after saving the title is `t1`, status "Saved t1.mirae". |
| Hide, show a layer | `t1*` after hiding, still `t1*` after showing (F3). |
| Ctrl+N on a modified document | Prompt "“Untitled” has unsaved changes. Save them before creating a new document?" with Save / Don't Save / Cancel. Cancel: nothing changes. Save: the save dialog, then the new document. Don't Save: the new document. |
| Ctrl+O | Native open dialog; the saved stroke and layers come back; history empty (Undo disabled). |
| Opening a truncated `.mirae` | Notice "Could not open “cut.mirae”. the file is not a Mirae document, or it is damaged" (wording as of this run); the open document unchanged. |
| Import PNG (button) | Layer `rgba8` above the active layer, centred, status "Imported rgba8.png (244 × 244)", one undo step. |
| Export PNG, transparent, with a Screen layer | Notice "not supported: a transparent export of layers in blend modes other than Normal … export with the paper instead"; no file, no temporary file. |
| Drag-and-drop | A dropped `.png` became a layer; a dropped `.mirae` on a modified document asked first, and Don't Save opened it. |
| Close (Alt+F4) on a modified document | The prompt; Cancel keeps the window; Don't Save closes it. |
| 16-bit document with a clipped, alpha-locked layer with a mask, saved and reopened | Reopened as 16-bit, the layer clipped (`>`), alpha-locked (`α`), its mask enabled and still the edit target; the clipped stroke shows only over its base. |
| Ctrl+S on an opened file, then undo and redo | The file was rewritten in place; undo → `c16*`, redo → `c16`. |
| Synthetic pen (`inject_pen "Mirae" pressure`) on a loaded document | Strokes arrive as pen input. |

A first implementation used native message boxes for the prompt; in the
running app they never appeared (a message box opened inside winit's
event handler waits for an idle queue that never comes) and the app
looked frozen. The prompt is now an egui modal (ADR 0020).

### Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-4F-1 | Mirae edits one document per window: New, Open and a dropped `.mirae` replace it (after the unsaved-changes prompt), where the reference opens a tab and asks nothing (F8, F9). | Behavioural (architecture) | Accepted |
| D-4F-2 | Undoing back to the saved state is unmodified in Mirae; the reference stays modified (F4). | Behavioural | Accepted — deliberate improvement (owner's brief) |
| D-4F-3 | 16-bit PNG import: Mirae keeps 16 bits (16-bit documents) or rounds to the nearest level (8-bit documents); the reference truncates (`v >> 8`, F18). Measured: one level in 22 of 80 swatches. Resolves D-4E-2. | Quality | Accepted — deliberate quality difference, not a parity error |
| D-4F-4 | 8-bit documents store premultiplied 8-bit pixels (ADR 0001): an imported colour at very low alpha changes (up to 127 levels at alpha 1; exact when opaque; alpha exact). The reference keeps it exactly (F16), as Mirae's 16-bit documents do. | Quality | Accepted (ADR 0001); 16-bit documents for such material |
| D-4F-5 | Transparent export of a layer whose non-Normal mode blends with the stack is refused (the reference's rule over a translucent backdrop is D-4C-1). Export with the paper is unaffected. | Feature | **Resolved in 5B** (exported, ADR 0022) |
| D-4F-6 | PNG metadata: Mirae writes an `sRGB` chunk only; the reference embeds an sRGB ICC profile, a 72 dpi `pHYs` and `tEXt` (F13). Fully transparent pixels: Mirae (0,0,0,0), the reference (255,255,255,0). Visible pixels equal. | Coverage | Accepted (no colour management or resolution yet) |
| D-4F-7 | An imported image becomes an ordinary raster layer; the reference creates an image material layer with transform handles (F15). | Feature | Open (transform tools, phase 6) |
| D-4F-8 | Not in Mirae: Recent files, Save copy, Revert, Close (single document), export to other formats, PSD, opening a PNG as a new document (F8, F11). | Feature | Open |
| D-4F-9 | Prompt wording and labels are Mirae's own English ("Save / Don't Save / Cancel"); the reference's quit prompt counts several documents (F10). | UI | Accepted |
| D-4F-10 | Corrupt input: the reference reports a corrupt `.clip` as an unsupported format and **crashed** on a corrupt PNG (F19, F20); Mirae names the problem and keeps the document. | Robustness | Accepted (Mirae must not crash) |
| D-4F-11 | Not compared: drag-and-drop in the reference, closing a clean document, file speed (Mirae measured only, `io_bench`), what the reference's format keeps of masks and clipping. | Coverage | Open |

## Update 5B

D-4F-5 is **resolved**: with the translucent-backdrop rule measured
(ADR 0022), a transparent PNG export composes without the paper
(`compose_tile_transparent_into`) and blend layers are exported instead
of refused. Exports with the paper and Normal-only transparent exports
are unchanged.
