//! The selection's outline: the boundary of the pixels at least half
//! covered (coverage ≥ 128), as the reference draws its marching ants
//! (parity record 5C, S31). Presentation data only: computed from the
//! coverage on demand, never stored in the document.

use std::collections::HashMap;

use tela_canvas::{IRect, TILE_SIZE};

use super::Selection;

/// A pixel corner, relative to the outline's area.
type Vertex = (i32, i32);

/// A pixel counts as inside at this coverage and above.
pub const OUTLINE_THRESHOLD: u8 = 128;

impl Selection {
    /// Closed loops along pixel edges around the pixels with coverage ≥
    /// [`OUTLINE_THRESHOLD`], in document pixel-corner coordinates,
    /// clockwise on screen (inside on the right), with collinear points
    /// merged. Holes are loops of their own. Pixels touching only at a
    /// corner belong to separate loops. Deterministic.
    pub fn outline(&self) -> Vec<Vec<[i32; 2]>> {
        let area = self.bounds();
        if area.is_empty() {
            return Vec::new();
        }
        let inside = self.inside_bitmap(area);
        let (w, h) = (area.width(), area.height());
        let at =
            |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && inside[(y * w + x) as usize];
        // Directed unit edges, inside on the right (y down).
        let mut next: HashMap<Vertex, [Option<Vertex>; 2]> = HashMap::new();
        let mut add = |from: (i32, i32), to: (i32, i32)| {
            let slot = next.entry(from).or_default();
            if slot[0].is_none() {
                slot[0] = Some(to);
            } else {
                slot[1] = Some(to);
            }
        };
        let mut starts = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if !at(x, y) {
                    continue;
                }
                if !at(x, y - 1) {
                    add((x, y), (x + 1, y));
                    starts.push((x, y));
                }
                if !at(x + 1, y) {
                    add((x + 1, y), (x + 1, y + 1));
                }
                if !at(x, y + 1) {
                    add((x + 1, y + 1), (x, y + 1));
                }
                if !at(x - 1, y) {
                    add((x, y + 1), (x, y));
                }
            }
        }
        let mut loops = Vec::new();
        for start in starts {
            if next
                .get(&start)
                .is_none_or(|s| s.iter().all(Option::is_none))
            {
                continue;
            }
            let mut points = vec![start];
            let mut at_point = start;
            // As if arriving up the left edge: the first move is the top
            // edge this loop starts on.
            let mut dir = (0, -1);
            loop {
                let slot = next.get_mut(&at_point).expect("every vertex continues");
                // At a corner shared by two loops take the right turn, so
                // diagonal neighbours stay apart.
                let right = (-dir.1, dir.0);
                let pick = match (slot[0], slot[1]) {
                    (Some(a), Some(b)) => {
                        let da = (a.0 - at_point.0, a.1 - at_point.1);
                        usize::from(da != right && (b.0 - at_point.0, b.1 - at_point.1) == right)
                    }
                    (Some(_), None) => 0,
                    _ => 1,
                };
                let to = slot[pick].take().expect("an outgoing edge");
                dir = (to.0 - at_point.0, to.1 - at_point.1);
                at_point = to;
                if at_point == start {
                    break;
                }
                points.push(at_point);
            }
            loops.push(simplify(&points, area));
        }
        loops
    }

    /// Whether each pixel of `area` is at least half covered, row-major.
    fn inside_bitmap(&self, area: IRect) -> Vec<bool> {
        let (w, h) = (area.width() as usize, area.height() as usize);
        let mut inside = vec![false; w * h];
        for c in area.tiles() {
            let part = c.rect().intersect(&area);
            let coverage = self.tile(c);
            let (ox, oy) = c.origin();
            for y in part.y0..part.y1 {
                let row = (y - area.y0) as usize * w;
                for x in part.x0..part.x1 {
                    let v = coverage.get((x - ox) as u32, (y - oy) as u32);
                    inside[row + (x - area.x0) as usize] = v >= OUTLINE_THRESHOLD;
                }
            }
            debug_assert!(part.width() <= TILE_SIZE as i32);
        }
        inside
    }
}

/// Drops points between collinear neighbours and moves the loop to
/// document coordinates.
fn simplify(points: &[(i32, i32)], area: IRect) -> Vec<[i32; 2]> {
    let n = points.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (p, q, r) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
        let collinear = (q.0 - p.0) * (r.1 - q.1) == (q.1 - p.1) * (r.0 - q.0);
        if !collinear {
            out.push([q.0 + area.x0, q.1 + area.y0]);
        }
    }
    out
}
