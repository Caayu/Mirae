//! Device units to normalized units. Pure functions, unit tested.

use glam::DVec2;

/// Windows Ink reports pressure in `0..=1024`.
pub const INK_PRESSURE_MAX: u32 = 1024;

pub fn ink_pressure(raw: u32) -> f32 {
    (raw as f32 / INK_PRESSURE_MAX as f32).clamp(0.0, 1.0)
}

/// Tilt in whole degrees (`-90..=90`) to radians.
pub fn tilt_degrees(raw: i32) -> f32 {
    (raw.clamp(-90, 90) as f32).to_radians()
}

/// Rotation in whole degrees (`0..=359`) to radians in `0..2π`.
pub fn rotation_degrees(raw: u32) -> f32 {
    ((raw % 360) as f32).to_radians()
}

/// Maps a device's HIMETRIC coordinates onto the screen rectangle it drives,
/// recovering the sub-pixel precision that integer pixel positions lose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HimetricMapper {
    device_min: DVec2,
    device_size: DVec2,
    screen_min: DVec2,
    screen_size: DVec2,
}

impl HimetricMapper {
    /// Rectangles as `(left, top, right, bottom)`. `None` if degenerate.
    pub fn new(device: (i32, i32, i32, i32), screen: (i32, i32, i32, i32)) -> Option<Self> {
        let d = |(l, t, r, b): (i32, i32, i32, i32)| {
            (
                DVec2::new(l as f64, t as f64),
                DVec2::new((r - l) as f64, (b - t) as f64),
            )
        };
        let (device_min, device_size) = d(device);
        let (screen_min, screen_size) = d(screen);
        (device_size.min_element() > 0.0 && screen_size.min_element() > 0.0).then_some(Self {
            device_min,
            device_size,
            screen_min,
            screen_size,
        })
    }

    pub fn to_screen(&self, himetric: (i32, i32)) -> DVec2 {
        let h = DVec2::new(himetric.0 as f64, himetric.1 as f64);
        self.screen_min + (h - self.device_min) / self.device_size * self.screen_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_is_unit_range() {
        assert_eq!(ink_pressure(0), 0.0);
        assert_eq!(ink_pressure(512), 0.5);
        assert_eq!(ink_pressure(1024), 1.0);
        assert_eq!(ink_pressure(5000), 1.0);
    }

    #[test]
    fn angles() {
        assert!((tilt_degrees(45) - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert!((tilt_degrees(-200) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(rotation_degrees(360), 0.0);
        assert!((rotation_degrees(180) - std::f32::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn himetric_maps_corners_and_keeps_subpixels() {
        let m = HimetricMapper::new((0, 0, 21600, 13500), (1920, 0, 4480, 1600)).unwrap();
        assert_eq!(m.to_screen((0, 0)), DVec2::new(1920.0, 0.0));
        assert_eq!(m.to_screen((21600, 13500)), DVec2::new(4480.0, 1600.0));
        let p = m.to_screen((1, 1));
        assert!(
            p.x.fract() > 0.0,
            "one HIMETRIC unit is a fraction of a pixel"
        );
    }

    #[test]
    fn degenerate_rects_are_rejected() {
        assert!(HimetricMapper::new((0, 0, 0, 10), (0, 0, 10, 10)).is_none());
        assert!(HimetricMapper::new((0, 0, 10, 10), (5, 5, 5, 9)).is_none());
    }
}
