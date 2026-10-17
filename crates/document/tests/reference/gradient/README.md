# Reference measurements: gradients (phase 5F)

The reference application (CLIP STUDIO PAINT EX 5.1.5) drawing gradients
on **our own** test documents (`cargo run -p tela-corpus --example
gradient_probe -- psd <kind> <path>`, see
`crates/corpus/examples/gradient_probe.rs`): 400 × 300 canvases with an
opaque black layer, an opaque white layer above it and a layer "ink" that
the gradient tool ("Degradê") edits. Procedure and observations:
docs/parity/5F-gradients.md (GD1–GD40).

Every PNG is **decoded**: two captures at 100 % zoom, over the white layer
and over the black one (`gradient_probe decode`), give the ink layer's
premultiplied RGBA, stored as the PNG's bytes (not a straight-alpha PNG).
Exception: in `mask_*` the ink layer is opaque grey 128 and the decoded
alpha is the mask's reveal value.

Unless stated, the sub-tool is "Primeiro plano para plano de fundo"
(main → sub colour), main black, sub white, dithering ("Trama") on, edge
processing "Não repetir", shape "Linha reta", and the drag runs from
document (100, 150) to (300, 150) with the mouse.

| File | Mixing mode (brightness correction), settings, drag |
|---|---|
| `std_bw`, `lps_bw`, `pps_bw` | Padrão (Standard); Linear (compatível com Photoshop); Perceptiva (compatível com Photoshop) |
| `per0_bw`, `per1_bw`, `per2_bw`, `per3_bw`, `per4_bw` | Perceptiva with brightness correction Desativada, Baixa, Média, **Alta (the default)**, A mais alta |
| `per3_wb` | Perceptiva/Alta, main white, sub black |
| `std_rb`, `lps_rb`, `pps_rb`, `per0_rb`, `per3_rb` | main red (255, 0, 0), sub blue (0, 0, 255), in the modes above |
| `ms_rbw_std`, `ms_rbw_std_nodither` | Padrão, three stops: red at 0, (255, 128, 128) at 50, white at 100 (all opacity 100); dithering on / off |
| `g_nodraw` | Padrão, edge "Não desenhar" |
| `z400_nodraw` | Padrão, "Não desenhar", dragged at **400 % zoom** from screen x 1198 to 1358 (document ≈ 181.7 → 221.7), captured back at 100 % |
| `g_repeat`, `g_reverse` | Padrão, edge "Repetir" / "Inverter", drag (150, 150) → (250, 150) |
| `g_diag` | Padrão, drag (100, 50) → (300, 250) |
| `g_circle` | Padrão, shape Círculo, drag (150, 100) → (250, 200) |
| `g_circle2` | Padrão, Círculo, drag (200, 150) → (300, 190) |
| `g_ellipse` | Padrão, Elipse, drag (200, 150) → (300, 200), then a click at the drag's end (rotation 0) |
| `ft_red` | "Primeiro plano para transparente" (red at 0, white at **opacity 0** at 100), Perceptiva/Alta, main red |
| `ft_w50`, `ft_w50_std` | the same with the white stop at opacity 50; Perceptiva/Alta and Padrão |
| `ft_w1_std` | the white stop at opacity 1, Padrão |
| `over_before`, `over_ft`, `over_ft_alock` | `over` document before, after "Primeiro plano para transparente" (black) dragged (0, 280) → (400, 280), and the same with the layer's alpha lock |
| `sel_hard`, `sel_feather` | `plain`, rectangle selection (150, 100)–(250, 200), hard / feathered 10 px ("Desfocar borda"), "Primeiro plano para transparente" (black), drag (100, 30) → (300, 30) |
| `mask_ft`, `mask_bw` | `mask` document, the ink layer's mask (hides everything) as the target; "Primeiro plano para transparente" (black) / main black → sub white |
| `mode_raw` | `mode` document (ink in Multiply at 50 %): the gradient, then the layer set to Normal 100 % and "under" hidden |
| `clip_raw` | `clip` document (ink clipped to a disc): the gradient, then the clipping turned off |
| `dark_long` | Perceptiva/Alta, main black, sub (20, 20, 20), drag (0, 150) → (400, 150) |
| `outside` | Perceptiva/Alta, drag (−100, 150) → (500, 150) (starting outside the canvas) |
| `shift_drag` | Perceptiva/Alta, drag (100, 100) → (300, 130) with Shift held |

Nothing here comes from the reference application's files: these are its
observed results on our inputs.
