//! Site geometry. The polygon-containment test here is one of the three
//! independent geofence enforcement points (spec §3.2): the Governor uses it
//! to refuse authorization, the asset watchdog uses it to hold at the line.
//! It is deliberately conservative: on any doubt, a point is OUTSIDE.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn dist(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polygon(pub Vec<Point>);

impl Polygon {
    pub fn from_pairs(pairs: &[[f64; 2]]) -> Self {
        Polygon(pairs.iter().map(|p| Point::new(p[0], p[1])).collect())
    }

    /// Ray-cast containment. Points on the boundary count as inside — an
    /// asset holding exactly at the fence line is holding, not breaching.
    pub fn contains(&self, p: &Point) -> bool {
        let n = self.0.len();
        if n < 3 {
            return false; // degenerate fence contains nothing: fail closed
        }
        // Boundary check first: within epsilon of any edge counts as inside.
        for i in 0..n {
            let a = self.0[i];
            let b = self.0[(i + 1) % n];
            if point_segment_distance(p, &a, &b) < 1e-9 {
                return true;
            }
        }
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (pi, pj) = (self.0[i], self.0[j]);
            if ((pi.y > p.y) != (pj.y > p.y))
                && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x)
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// True iff every vertex of `inner` is inside `self` AND no edge of
    /// `inner` crosses an edge of `self`. For simple polygons this is
    /// containment; anything degenerate returns false (fail closed).
    pub fn contains_polygon(&self, inner: &Polygon) -> bool {
        if inner.0.is_empty() || self.0.len() < 3 {
            return false;
        }
        if !inner.0.iter().all(|p| self.contains(p)) {
            return false;
        }
        let ni = inner.0.len();
        let no = self.0.len();
        if ni >= 2 {
            for i in 0..ni {
                let a1 = inner.0[i];
                let a2 = inner.0[(i + 1) % ni];
                for j in 0..no {
                    let b1 = self.0[j];
                    let b2 = self.0[(j + 1) % no];
                    if segments_properly_intersect(&a1, &a2, &b1, &b2) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Where the segment a→b first crosses this polygon's boundary, as the
    /// parameter t in [0,1] along a→b. None if it never crosses.
    pub fn first_boundary_crossing(&self, a: &Point, b: &Point) -> Option<f64> {
        let n = self.0.len();
        let mut best: Option<f64> = None;
        for i in 0..n {
            let c = self.0[i];
            let d = self.0[(i + 1) % n];
            if let Some(t) = segment_intersection_t(a, b, &c, &d) {
                best = Some(best.map_or(t, |cur: f64| cur.min(t)));
            }
        }
        best
    }
}

fn point_segment_distance(p: &Point, a: &Point, b: &Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.dist(&Point::new(a.x + t * dx, a.y + t * dy))
}

fn cross(o: &Point, a: &Point, b: &Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Proper intersection: the segments cross at a single interior point.
/// Shared endpoints / collinear touching do not count (an inner polygon may
/// touch the fence from inside without breaching it).
fn segments_properly_intersect(a1: &Point, a2: &Point, b1: &Point, b2: &Point) -> bool {
    let d1 = cross(b1, b2, a1);
    let d2 = cross(b1, b2, a2);
    let d3 = cross(a1, a2, b1);
    let d4 = cross(a1, a2, b2);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Parameter t along a→b where it intersects segment c→d, if it does.
fn segment_intersection_t(a: &Point, b: &Point, c: &Point, d: &Point) -> Option<f64> {
    let r = Point::new(b.x - a.x, b.y - a.y);
    let s = Point::new(d.x - c.x, d.y - c.y);
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((c.x - a.x) * s.y - (c.y - a.y) * s.x) / denom;
    let u = ((c.x - a.x) * r.y - (c.y - a.y) * r.x) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fence() -> Polygon {
        Polygon::from_pairs(&[[15.0, 15.0], [785.0, 15.0], [785.0, 385.0], [15.0, 385.0]])
    }

    #[test]
    fn contains_interior_point() {
        assert!(fence().contains(&Point::new(400.0, 200.0)));
    }

    #[test]
    fn excludes_exterior_point() {
        assert!(!fence().contains(&Point::new(845.0, 102.0)));
    }

    #[test]
    fn boundary_counts_as_inside() {
        assert!(fence().contains(&Point::new(785.0, 200.0)));
    }

    #[test]
    fn degenerate_polygon_contains_nothing() {
        let deg = Polygon::from_pairs(&[[0.0, 0.0], [1.0, 1.0]]);
        assert!(!deg.contains(&Point::new(0.5, 0.5)));
    }

    #[test]
    fn polygon_containment() {
        let inner = Polygon::from_pairs(&[
            [420.0, 110.0],
            [450.0, 110.0],
            [450.0, 250.0],
            [420.0, 250.0],
        ]);
        assert!(fence().contains_polygon(&inner));
        // Clips 2 m outside the east fence → not contained (G-04).
        let clipping = Polygon::from_pairs(&[
            [700.0, 100.0],
            [787.0, 100.0],
            [787.0, 120.0],
            [700.0, 120.0],
        ]);
        assert!(!fence().contains_polygon(&clipping));
        // Empty envelope: fail closed.
        assert!(!fence().contains_polygon(&Polygon(vec![])));
    }

    #[test]
    fn first_crossing_on_flee_path() {
        let t = fence()
            .first_boundary_crossing(&Point::new(730.0, 120.0), &Point::new(860.0, 95.0))
            .expect("path crosses the fence");
        let x = 730.0 + t * (860.0 - 730.0);
        assert!((x - 785.0).abs() < 1e-6);
    }
}
