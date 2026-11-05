//! Geometric shapes: a trait with a default method and two implementations.

use std::f64::consts::PI;
use std::fmt;

/// Something with an area.
pub trait Shape: fmt::Debug {
    /// The area in square units.
    fn area(&self) -> f64;

    /// A short human-readable description.
    fn describe(&self) -> String {
        format!("{:?} with area {:.2}", self, self.area())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Circle {
    pub radius: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect(pub f64, pub f64);

impl Circle {
    pub const UNIT: Circle = Circle { radius: 1.0 };

    pub fn new(radius: f64) -> Self {
        Self {
            radius: radius.abs(),
        }
    }
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        PI * self.radius * self.radius
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.0 * self.1
    }

    fn describe(&self) -> String {
        if self.0 == self.1 {
            format!("square {}", self.0)
        } else {
            format!("rect {}x{}", self.0, self.1)
        }
    }
}

impl fmt::Display for Circle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "circle(r={})", self.radius)
    }
}

/// The largest shape by area.
pub fn largest(shapes: &[Box<dyn Shape>]) -> Option<&dyn Shape> {
    shapes
        .iter()
        .map(|s| s.as_ref())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
}
