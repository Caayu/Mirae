use crate::tile::{TILE_SIZE, TileCoord};

/// Half-open integer rectangle `[x0, x1) x [y0, y1)` in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub const fn from_size(width: u32, height: u32) -> Self {
        Self::new(0, 0, width as i32, height as i32)
    }

    /// Smallest integer rectangle containing the real-valued box.
    pub fn enclosing(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self::new(
            min_x.floor() as i32,
            min_y.floor() as i32,
            max_x.ceil() as i32,
            max_y.ceil() as i32,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    pub fn width(&self) -> i32 {
        (self.x1 - self.x0).max(0)
    }

    pub fn height(&self) -> i32 {
        (self.y1 - self.y0).max(0)
    }

    pub fn intersect(&self, o: &Self) -> Self {
        Self::new(
            self.x0.max(o.x0),
            self.y0.max(o.y0),
            self.x1.min(o.x1),
            self.y1.min(o.y1),
        )
    }

    pub fn union(&self, o: &Self) -> Self {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Self::new(
            self.x0.min(o.x0),
            self.y0.min(o.y0),
            self.x1.max(o.x1),
            self.y1.max(o.y1),
        )
    }

    /// Every tile that overlaps this rectangle, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = TileCoord> + use<> {
        let (tx0, ty0, tx1, ty1) = if self.is_empty() {
            (0, 0, 0, 0)
        } else {
            let t = TILE_SIZE as i32;
            (
                self.x0.div_euclid(t),
                self.y0.div_euclid(t),
                (self.x1 - 1).div_euclid(t) + 1,
                (self.y1 - 1).div_euclid(t) + 1,
            )
        };
        (ty0..ty1).flat_map(move |y| (tx0..tx1).map(move |x| TileCoord { x, y }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_exactly_the_overlapping_tiles() {
        let r = IRect::new(250, 0, 260, 10);
        let t: Vec<_> = r.tiles().collect();
        assert_eq!(t, vec![TileCoord { x: 0, y: 0 }, TileCoord { x: 1, y: 0 }]);

        let r = IRect::new(0, 0, 256, 256);
        assert_eq!(r.tiles().count(), 1);

        let r = IRect::new(-1, -1, 1, 1);
        assert_eq!(r.tiles().count(), 4);
    }

    #[test]
    fn empty_rect_has_no_tiles() {
        assert_eq!(IRect::new(5, 5, 5, 9).tiles().count(), 0);
    }

    #[test]
    fn union_ignores_empty() {
        let a = IRect::new(0, 0, 4, 4);
        assert_eq!(a.union(&IRect::default()), a);
        assert_eq!(IRect::default().union(&a), a);
    }

    #[test]
    fn enclosing_rounds_outward() {
        assert_eq!(
            IRect::enclosing(-0.5, 1.2, 3.1, 4.0),
            IRect::new(-1, 1, 4, 4)
        );
    }
}
