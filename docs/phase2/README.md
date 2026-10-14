# Phase 2 — tablet input and stroke quality

## Pipeline

```
Windows Ink (WM_POINTER + history) ─┐
mouse (winit) ──────────────────────┴─> TabletSample            [tela-tablet]
  -> unit normalization -> clock (QPC) -> TimestampNormalizer
  -> screen->canvas mapping (app)
  -> BrushInput                                                 [tela-brush]
  -> pressure curve -> velocity -> jitter filter -> stabilizer
  -> interpolator -> dab spacing -> dynamics & taper -> DabSink
  -> StrokeBuffer (committed mask + re-renderable tail) -> dirty tiles
```

## Stabilizer benchmark

`cargo run --release -p tela-brush --example stabilizer_bench`
(synthetic corpus: 200 Hz, 0.4 px sensor noise, 0.5 px 9 Hz tremor, 0.5 ms timing jitter)

See [stabilizer_bench.md](stabilizer_bench.md). One Euro is the default because no
setting of the alternatives reaches both lower jitter and lower lag
(`tests/stabilizer_comparison.rs`, Pareto test).

## Interpolation benchmark

`cargo run --release -p tela-brush --example interpolation_bench`
(noise-free, sparsely sampled strokes)

See [interpolation_bench.md](interpolation_bench.md). Centripetal Catmull–Rom is
the default: same smoothness as the Bézier variant, tighter on abrupt direction
changes, cusp-free by construction.

## Evidence from the running app

| File | What it shows |
|---|---|
| `mouse_suite.png` | mouse: slow/fast lines, slow/fast circle (18 samples), diagonals, S, small loops, spiral, fast V |
| `mouse_rotated.png` | strokes on a 30° rotated canvas |
| `mouse_zoom.png` | strokes at high zoom |
| `pen_all.png` | synthetic Windows Ink pen: pressure sweep, S with tilt/rotation, fast circle, diagonals, V, spiral |
| `pen_pressure_big.png` | pressure 0.05→1→0.05 and 0.3→0.9 with a 57 px brush: no steps |
| `stab_compare.png` | same ±2 px noisy input: stabilization 0 (top) vs 80 + tapers (bottom) |

The pen strokes were produced by `crates/tablet/examples/inject_pen.rs`, which
injects pen frames through `InjectSyntheticPointerInput`, so they travel the
real `WM_POINTER` path (history, pen masks, timestamps). Synthetic injection
only accepts integer pixel positions, so it does not exercise the HIMETRIC
sub-pixel mapping.

## Latency (release build, this machine)

| Source | Rate | Input age avg | Pipeline / sample | Compose / frame | Sample → GPU submit avg |
|---|---|---|---|---|---|
| Synthetic pen (Windows Ink) | ~220–230 Hz | ~3 ms | 15–70 µs | 0.2–0.7 ms | ~9–10 ms |
| Mouse (scripted) | ~64 Hz | ~0 ms¹ | 15–50 µs | 0.2–0.6 ms | ~1–2 ms¹ |

¹ winit does not expose mouse device timestamps; mouse samples are stamped on
arrival, so their age is underestimated. Pen timestamps come from the
performance counter stamped by Windows.

Enable with `RUST_LOG=info,tela::latency=debug`; press F3 for a title-bar
summary.
