//! sRGB transfer functions (IEC 61966-2-1).

/// Encoded sRGB value to linear light.
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to encoded sRGB value.
pub fn from_linear(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for i in 0..=100 {
            let c = i as f32 / 100.0;
            assert!((from_linear(to_linear(c)) - c).abs() < 1e-5, "{c}");
        }
    }

    #[test]
    fn known_values() {
        assert_eq!(to_linear(0.0), 0.0);
        assert!((to_linear(1.0) - 1.0).abs() < 1e-6);
        // Mid-grey 0.5 encoded is ~21.4% linear light.
        assert!((to_linear(0.5) - 0.214_04).abs() < 1e-4);
    }
}
