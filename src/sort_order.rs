use crate::cycle::Cycle;
use crate::scanner::CruftDirectory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    SizeDescending,
    AgeDescending,
    Alphabetical,
}

// Implement the Cycle trait for SortOrder
impl Cycle for SortOrder {
    fn all_values() -> &'static [Self] {
        static ALL: [SortOrder; 3] = [
            SortOrder::SizeDescending,
            SortOrder::AgeDescending,
            SortOrder::Alphabetical
        ];
        &ALL
    }
}

impl SortOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortOrder::SizeDescending => "size",
            SortOrder::AgeDescending => "age",
            SortOrder::Alphabetical => "name",
        }
    }

    pub fn sort_entries(&self, entries: &mut Vec<CruftDirectory>) {
        match self {
            SortOrder::SizeDescending => {
                entries.sort_by(|a, b| b.size.cmp(&a.size));
            },
            SortOrder::AgeDescending => {
                entries.sort_by(|a, b| b.newest_file_age_days.cmp(&a.newest_file_age_days));
            },
            SortOrder::Alphabetical => {
                entries.sort_by(|a, b| a.path.to_string_lossy().cmp(&b.path.to_string_lossy()));
            },
        }
    }
}