/// Coverage (`0..=1`) of a round tip at distance `dist` from its center.
///
/// Inside `hardness * radius` coverage is 1. Beyond it, coverage falls off with
/// a smoothstep curve that reaches 0 just outside the radius. With
/// anti-aliasing the ramp is always at least one pixel wide, so a fully hard
/// brush gets a 1 px box-filter-like edge instead of a staircase; without it a
/// fully hard brush is a binary disc.
pub fn round_tip_coverage(dist: f32, radius: f32, hardness: f32, anti_aliasing: bool) -> f32 {
    let aa = if anti_aliasing { 0.5 } else { 0.0 };
    let outer = radius + aa;
    let inner = (radius * hardness.clamp(0.0, 1.0)).min(radius - aa);
    // Outer test first: without anti-aliasing inner == outer and the boundary
    // pixel must be excluded.
    if dist >= outer {
        return 0.0;
    }
    if dist <= inner {
        return 1.0;
    }
    let t = (dist - inner) / (outer - inner);
    1.0 - t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_is_full_and_far_is_empty() {
        for h in [0.0, 0.5, 1.0] {
            assert_eq!(round_tip_coverage(0.0, 10.0, h, true), 1.0);
            assert_eq!(round_tip_coverage(11.0, 10.0, h, true), 0.0);
        }
    }

    #[test]
    fn coverage_is_monotonic_in_distance() {
        for h in [0.0, 0.3, 0.9, 1.0] {
            let mut prev = 1.0;
            for i in 0..=120 {
                let c = round_tip_coverage(i as f32 * 0.1, 10.0, h, true);
                assert!(c <= prev + 1e-6);
                prev = c;
            }
        }
    }

    #[test]
    fn hard_aa_edge_is_one_pixel_wide_and_centered() {
        assert_eq!(round_tip_coverage(9.5, 10.0, 1.0, true), 1.0);
        assert!((round_tip_coverage(10.0, 10.0, 1.0, true) - 0.5).abs() < 1e-6);
        assert_eq!(round_tip_coverage(10.5, 10.0, 1.0, true), 0.0);
    }

    #[test]
    fn hard_without_aa_is_binary() {
        assert_eq!(round_tip_coverage(9.99, 10.0, 1.0, false), 1.0);
        assert_eq!(round_tip_coverage(10.0, 10.0, 1.0, false), 0.0);
    }

    #[test]
    fn soft_brush_falls_off_from_center() {
        let half = round_tip_coverage(5.0, 10.0, 0.0, true);
        assert!(half > 0.3 && half < 0.8, "{half}");
    }
}
