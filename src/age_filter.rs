use crate::cycle::Cycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeFilter {
    None,
    Days90,
    Days180,
    Days365,
}

impl AgeFilter {
    pub fn as_days(&self) -> Option<u64> {
        match self {
            AgeFilter::None => None,
            AgeFilter::Days90 => Some(90),
            AgeFilter::Days180 => Some(180),
            AgeFilter::Days365 => Some(365),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            AgeFilter::None => "all",
            AgeFilter::Days90 => "90 days",
            AgeFilter::Days180 => "180 days",
            AgeFilter::Days365 => "365 days",
        }
    }
}

// Implement the Cycle trait for AgeFilter
impl Cycle for AgeFilter {
    fn all_values() -> &'static [Self] {
        static ALL: [AgeFilter; 4] = [
            AgeFilter::None,
            AgeFilter::Days90,
            AgeFilter::Days180,
            AgeFilter::Days365,
        ];
        &ALL
    }
}
