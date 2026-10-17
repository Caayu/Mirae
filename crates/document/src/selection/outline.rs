//! The selection's outline: the boundary of the pixels at least half
//! covered (coverage ≥ 128), as the reference draws its marching ants
//! (parity record 5C, S31). Presentation data only: computed from the
//! coverage on demand, never stored in the document.

use std::collections::HashMap;

use tela_canvas::IRect;

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
        let none = vec![false; w as usize];
        for y in 0..h {
            let row = |y: i32| -> &[bool] {
                if (0..h).contains(&y) {
                    &inside[(y * w) as usize..((y + 1) * w) as usize]
                } else {
                    &none
                }
            };
            let (above, cur, below) = (row(y - 1), row(y), row(y + 1));
            let mut x = 0;
            while x < w {
                // A run of inside pixels [x, end): a left and a right edge,
                // top and bottom edges where the neighbour row is outside.
                let Some(skip) = cur[x as usize..].iter().position(|&v| v) else {
                    break;
                };
                x += skip as i32;
                let len = cur[x as usize..]
                    .iter()
                    .position(|&v| !v)
                    .unwrap_or((w - x) as usize);
                let end = x + len as i32;
                add((x, y + 1), (x, y));
                add((end, y), (end, y + 1));
                for px in x..end {
                    if !above[px as usize] {
                        add((px, y), (px + 1, y));
                        starts.push((px, y));
                    }
                    if !below[px as usize] {
                        add((px + 1, y + 1), (px, y + 1));
                    }
                }
                x = end;
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
        let mut line = vec![0u8; w];
        for (y, out) in (area.y0..area.y1).zip(inside.chunks_exact_mut(w)) {
            self.read_line(area.x0, y, &mut line);
            for (o, &v) in out.iter_mut().zip(&line) {
                *o = v >= OUTLINE_THRESHOLD;
            }
        }
        debug_assert_eq!(inside.len(), w * h);
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
