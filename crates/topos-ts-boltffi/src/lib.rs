use boltffi::*;

#[data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn distance(a: Point, b: Point) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}
