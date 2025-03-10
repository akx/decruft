// A trait for cycling through enum variants
pub trait Cycle: Sized + Copy + PartialEq + 'static {
    // All possible values in order
    fn all_values() -> &'static [Self];

    // Get the next value in the cycle
    fn next(&self) -> Self {
        let all = Self::all_values();
        let current_idx = all.iter()
            .position(|&val| val == *self)
            .unwrap_or(0);

        let next_idx = (current_idx + 1) % all.len();
        all[next_idx]
    }
}