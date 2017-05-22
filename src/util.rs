/// Absolute difference of two numbers.
pub fn abs_diff(x: usize, y: usize) -> usize {
    if x >= y { x - y } else { y - x }
}
