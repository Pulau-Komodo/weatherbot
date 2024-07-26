/// Convert a `f32` into a `i32` and multiply it by 100, because the graph drawing library uses them this way often.
pub fn convert_num(n: f32) -> i32 {
	(n * 100.0).round() as i32
}
