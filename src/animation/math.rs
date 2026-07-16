use std::ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vector2 {
    pub x: f64,
    pub y: f64,
}

impl From<f64> for Vector2 {
    fn from(value: f64) -> Self {
        Vector2 { x: value, y: value }
    }
}

impl From<(f64, f64)> for Vector2 {
    fn from((x, y): (f64, f64)) -> Self {
        Vector2 { x, y }
    }
}

impl Add<Vector2> for Vector2 {
    type Output = Vector2;

    fn add(self, rhs: Vector2) -> Self::Output {
        Vector2 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub<Vector2> for Vector2 {
    type Output = Vector2;

    fn sub(self, rhs: Vector2) -> Self::Output {
        Vector2 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Add<f64> for Vector2 {
    type Output = Vector2;

    fn add(self, rhs: f64) -> Self::Output {
        self + Vector2::from(rhs)
    }
}

impl Sub<f64> for Vector2 {
    type Output = Vector2;

    fn sub(self, rhs: f64) -> Self::Output {
        self - Vector2::from(rhs)
    }
}

impl AddAssign<Vector2> for Vector2 {
    fn add_assign(&mut self, rhs: Vector2) {
        *self = *self + rhs;
    }
}

impl AddAssign<f64> for Vector2 {
    fn add_assign(&mut self, rhs: f64) {
        *self = *self + rhs;
    }
}

impl SubAssign<Vector2> for Vector2 {
    fn sub_assign(&mut self, rhs: Vector2) {
        *self = *self - rhs;
    }
}

impl SubAssign<f64> for Vector2 {
    fn sub_assign(&mut self, rhs: f64) {
        *self = *self - rhs;
    }
}

impl Mul<f64> for Vector2 {
    type Output = Vector2;

    fn mul(self, rhs: f64) -> Self::Output {
        Vector2 {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl MulAssign<f64> for Vector2 {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

pub fn sample_curve_x(t: f64, p1x: f64, p2x: f64) -> f64 {
    let cx = 3.0 * p1x;
    let bx = 3.0 * (p2x - p1x) - cx;
    let ax = 1.0 - cx - bx;
    ((ax * t + bx) * t + cx) * t
}

pub fn sample_curve_y(t: f64, p1y: f64, p2y: f64) -> f64 {
    let cy = 3.0 * p1y;
    let by = 3.0 * (p2y - p1y) - cy;
    let ay = 1.0 - cy - by;
    ((ay * t + by) * t + cy) * t
}

pub fn sample_curve_derivative_x(t: f64, p1x: f64, p2x: f64) -> f64 {
    let cx = 3.0 * p1x;
    let bx = 3.0 * (p2x - p1x) - cx;
    let ax = 1.0 - cx - bx;
    (3.0 * ax * t + 2.0 * bx) * t + cx
}

pub fn solve_curve_t(time_x: f64, p1x: f64, p2x: f64) -> f64 {
    let mut t2 = time_x;

    for _ in 0..8 {
        let current_x = sample_curve_x(t2, p1x, p2x) - time_x;
        if current_x.abs() < 1e-5 {
            return t2;
        }
        let derivative = sample_curve_derivative_x(t2, p1x, p2x);
        if derivative.abs() < 1e-6 {
            break;
        }
        t2 -= current_x / derivative;
    }

    // Fallback to binary search if Newton-Raphson fails
    t2.clamp(0.0, 1.0)
}
