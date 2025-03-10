use crate::cycle::Cycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeFilter {
    ShowAll,
    SkipSmall,
}

impl SizeFilter {
    pub fn as_str(&self) -> &'static str {
        match self {
            SizeFilter::ShowAll => "all",
            SizeFilter::SkipSmall => "skip small",
        }
    }

    pub fn as_bytes(&self) -> u64 {
        match self {
            SizeFilter::ShowAll => 0,
            SizeFilter::SkipSmall => 1_048_576, // 1 MB in bytes
        }
    }
}

// Implement the Cycle trait for SizeFilter
impl Cycle for SizeFilter {
    fn all_values() -> &'static [Self] {
        static ALL: [SizeFilter; 2] = [
            SizeFilter::ShowAll,
            SizeFilter::SkipSmall,
        ];
        &ALL
    }
}