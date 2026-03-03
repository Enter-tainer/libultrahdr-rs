pub mod gamut;
pub mod icc;
pub mod transfer;

use std::ops::{Add, AddAssign, Div, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn clamp01(self) -> Self {
        Self {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
        }
    }

    pub fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            r: f(self.r),
            g: f(self.g),
            b: f(self.b),
        }
    }
}

impl Add for Color {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            r: self.r + rhs.r,
            g: self.g + rhs.g,
            b: self.b + rhs.b,
        }
    }
}

impl AddAssign for Color {
    fn add_assign(&mut self, rhs: Self) {
        self.r += rhs.r;
        self.g += rhs.g;
        self.b += rhs.b;
    }
}

impl Sub for Color {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            r: self.r - rhs.r,
            g: self.g - rhs.g,
            b: self.b - rhs.b,
        }
    }
}

impl Mul<f32> for Color {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self {
            r: self.r * rhs,
            g: self.g * rhs,
            b: self.b * rhs,
        }
    }
}

impl Div<f32> for Color {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self {
            r: self.r / rhs,
            g: self.g / rhs,
            b: self.b / rhs,
        }
    }
}

impl Add<f32> for Color {
    type Output = Self;
    fn add(self, rhs: f32) -> Self {
        Self {
            r: self.r + rhs,
            g: self.g + rhs,
            b: self.b + rhs,
        }
    }
}

impl Sub<f32> for Color {
    type Output = Self;
    fn sub(self, rhs: f32) -> Self {
        Self {
            r: self.r - rhs,
            g: self.g - rhs,
            b: self.b - rhs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_add() {
        let a = Color {
            r: 1.0,
            g: 2.0,
            b: 3.0,
        };
        let b = Color {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        };
        let c = a + b;
        assert!((c.r - 1.5).abs() < 1e-7);
        assert!((c.g - 2.5).abs() < 1e-7);
        assert!((c.b - 3.5).abs() < 1e-7);
    }

    #[test]
    fn color_scale() {
        let a = Color {
            r: 1.0,
            g: 2.0,
            b: 3.0,
        };
        let c = a * 2.0;
        assert!((c.r - 2.0).abs() < 1e-7);
        assert!((c.g - 4.0).abs() < 1e-7);
        assert!((c.b - 6.0).abs() < 1e-7);
    }

    #[test]
    fn color_clamp() {
        let c = Color {
            r: -0.1,
            g: 0.5,
            b: 1.5,
        };
        let clamped = c.clamp01();
        assert!((clamped.r).abs() < 1e-7);
        assert!((clamped.g - 0.5).abs() < 1e-7);
        assert!((clamped.b - 1.0).abs() < 1e-7);
    }
}
