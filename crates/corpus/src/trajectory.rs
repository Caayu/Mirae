//! Standard trajectories, each fitting in one `CELL`-sized cell, sampled
//! like a 200 Hz pen with pressure.

use std::f32::consts::{PI, TAU};

use glam::Vec2;
use tela_brush::BrushInput;

pub const CELL: u32 = 320;
pub const COLUMNS: u32 = 4;
pub const ROWS: u32 = 3;
const RATE: f64 = 200.0;
const MARGIN: f32 = 50.0;

pub struct Trajectory {
    pub name: &'static str,
    pub strokes: Vec<Vec<Sample>>,
    /// Exercise the preset with tapers forced on.
    pub force_taper: bool,
    /// Seconds the pen rests after the last sample (airbrush build-up).
    pub hold_after: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub pos: Vec2,
    pub pressure: f32,
    pub time: f64,
}

impl Sample {
    pub fn translated(&self, origin: Vec2) -> BrushInput {
        BrushInput::from_position(origin + self.pos, self.time).with_pressure(self.pressure)
    }
}

/// Samples `path(u)` and `pressure(u)` over `duration` seconds, where the
/// curve parameter advances as `progress(t)` (identity = constant speed).
fn stroke(
    duration: f64,
    path: impl Fn(f32) -> Vec2,
    pressure: impl Fn(f32) -> f32,
    progress: impl Fn(f32) -> f32,
) -> Vec<Sample> {
    let n = ((duration * RATE).round() as usize).max(2);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = progress(t);
            Sample {
                pos: path(u),
                pressure: pressure(u).clamp(0.0, 1.0),
                time: i as f64 / RATE,
            }
        })
        .collect()
}

fn line(a: Vec2, b: Vec2) -> impl Fn(f32) -> Vec2 {
    move |u| a.lerp(b, u)
}

/// Pen-like pressure: rises, holds, releases.
fn natural(u: f32) -> f32 {
    0.35 + 0.6 * (PI * u).sin()
}

fn linear(t: f32) -> f32 {
    t
}

pub fn trajectories() -> Vec<Trajectory> {
    let c = CELL as f32;
    let (lo, hi) = (MARGIN, c - MARGIN);
    let mid = c / 2.0;
    let simple = |name, strokes| Trajectory {
        name,
        strokes,
        force_taper: false,
        hold_after: 0.0,
    };
    vec![
        simple(
            "horizontal line",
            vec![stroke(
                0.6,
                line(Vec2::new(lo, mid), Vec2::new(hi, mid)),
                natural,
                linear,
            )],
        ),
        simple(
            "diagonal line",
            vec![stroke(
                0.6,
                line(Vec2::new(lo, hi), Vec2::new(hi, lo)),
                natural,
                linear,
            )],
        ),
        simple(
            "S curve",
            vec![stroke(
                0.9,
                move |u| Vec2::new(mid + (mid - lo) * 0.6 * (TAU * u).sin(), lo + (hi - lo) * u),
                natural,
                linear,
            )],
        ),
        simple(
            "circle",
            vec![stroke(
                0.9,
                move |u| Vec2::splat(mid) + (mid - lo) * Vec2::from_angle(TAU * u - PI / 2.0),
                natural,
                linear,
            )],
        ),
        simple(
            "spiral",
            vec![stroke(
                1.5,
                move |u| {
                    Vec2::splat(mid)
                        + (8.0 + (mid - lo - 8.0) * u) * Vec2::from_angle(3.0 * TAU * u)
                },
                natural,
                linear,
            )],
        ),
        simple(
            "zigzag",
            vec![stroke(
                0.9,
                move |u| {
                    let tri = 1.0 - (2.0 * ((u * 4.0).fract()) - 1.0).abs();
                    Vec2::new(lo + (hi - lo) * u, lo + 40.0 + (hi - lo - 80.0) * tri)
                },
                |_| 0.8,
                linear,
            )],
        ),
        simple(
            "pressure ramp",
            vec![stroke(
                1.0,
                line(Vec2::new(lo, mid), Vec2::new(hi, mid)),
                |u| 0.02 + 0.98 * u,
                linear,
            )],
        ),
        simple(
            "velocity ramp",
            // Same path, accelerating: speed grows linearly with time.
            vec![stroke(
                0.5,
                line(Vec2::new(lo, mid), Vec2::new(hi, mid)),
                |_| 0.8,
                |t| t * t,
            )],
        ),
        simple("crosshatch", {
            let mut strokes = vec![];
            for k in 0..6 {
                let o = lo + k as f32 * (hi - lo) / 5.0;
                strokes.push(stroke(
                    0.25,
                    line(Vec2::new(o - 50.0, hi), Vec2::new(o + 50.0, lo)),
                    |_| 0.6,
                    linear,
                ));
                strokes.push(stroke(
                    0.25,
                    line(Vec2::new(lo, o + 50.0), Vec2::new(hi, o - 50.0)),
                    |_| 0.6,
                    linear,
                ));
            }
            strokes
        }),
        Trajectory {
            name: "taper",
            strokes: vec![stroke(
                0.8,
                move |u| Vec2::new(lo + (hi - lo) * u, mid + 60.0 * (PI * u).sin()),
                |_| 0.9,
                linear,
            )],
            force_taper: true,
            hold_after: 0.0,
        },
        Trajectory {
            name: "slow stroke",
            strokes: vec![stroke(
                2.5,
                move |u| Vec2::new(lo + (hi - lo) * u, mid + 30.0 * (TAU * u).sin()),
                natural,
                linear,
            )],
            force_taper: false,
            // A resting pen at the end shows airbrush build-up.
            hold_after: 0.5,
        },
        simple(
            "fast stroke",
            vec![stroke(
                0.08,
                move |u| Vec2::new(lo + (hi - lo) * u, mid - 60.0 * (PI * u).sin()),
                natural,
                linear,
            )],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_trajectories_fit_their_cells() {
        let t = trajectories();
        assert_eq!(t.len(), (COLUMNS * ROWS) as usize);
        for traj in &t {
            for s in traj.strokes.iter().flatten() {
                assert!(s.pos.x >= 0.0 && s.pos.x <= CELL as f32, "{}", traj.name);
                assert!(s.pos.y >= 0.0 && s.pos.y <= CELL as f32, "{}", traj.name);
                assert!((0.0..=1.0).contains(&s.pressure));
            }
            for st in &traj.strokes {
                assert!(st.windows(2).all(|w| w[1].time > w[0].time));
            }
        }
    }

    #[test]
    fn fast_stroke_is_sparse_and_slow_is_dense() {
        let t = trajectories();
        let count = |name: &str| t.iter().find(|x| x.name == name).unwrap().strokes[0].len();
        assert!(count("fast stroke") < 20);
        assert!(count("slow stroke") > 400);
    }
}
