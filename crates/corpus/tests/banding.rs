//! Phase 4E quality gate: 16-bit storage measurably reduces banding
//! (ADR 0018). The full table comes from
//! `cargo run --release -p tela-corpus --example banding`.

use tela_corpus::banding::{Metrics, repeated_strokes, soft_stroke, stored_ramp};

#[test]
fn repeated_faint_strokes_band_at_8_bits_and_not_at_16() {
    let stroke = soft_stroke(200.0, 0.03, 0.35);
    let eight = Metrics::of(&repeated_strokes::<[u8; 4]>(&stroke, 60));
    let sixteen = Metrics::of(&repeated_strokes::<[u16; 4]>(&stroke, 60));
    // 16 bits stays within one display level of the exact build-up…
    assert!(sixteen.max_error <= 1.0, "{sixteen:?}");
    assert_eq!(sixteen.lost, 0, "{sixteen:?}");
    // …8 bits misses it by many levels and drops the faint edge.
    assert!(eight.max_error >= 10.0, "{eight:?}");
    assert!(eight.lost >= 10, "{eight:?}");
    assert!(
        eight.max_jump > 4 * sixteen.max_jump,
        "{eight:?} {sixteen:?}"
    );
    assert!(sixteen.levels > eight.levels);
}

#[test]
fn a_single_stroke_shows_the_same_at_both_depths() {
    // One commit rounds once: the display is equally exact (≤ ½ level);
    // only the stored precision differs.
    let stroke = soft_stroke(200.0, 0.15, 0.35);
    let a = repeated_strokes::<[u8; 4]>(&stroke, 1);
    let b = repeated_strokes::<[u16; 4]>(&stroke, 1);
    assert_eq!(a.shown, b.shown);
    assert!(Metrics::of(&a).max_error <= 0.5 + 1e-9);
    assert!(b.stored_levels > a.stored_levels);
}

#[test]
fn storage_precision_is_not_display_precision() {
    let a = stored_ramp::<[u8; 4]>(0.0, 1.0, 4000);
    let b = stored_ramp::<[u16; 4]>(0.0, 1.0, 4000);
    assert_eq!(a.shown, b.shown, "an 8-bit display shows the same ramp");
    assert_eq!((a.stored_levels, b.stored_levels), (256, 4000));
}
