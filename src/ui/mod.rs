pub mod diffview;
pub mod filetree;
pub mod help;
pub mod prlist;
pub mod submit;

/// Marks the row the cursor is on. A marker plus bold rather than a reversed line, so a diff line
/// keeps the +/- color that says whether it was added or removed
pub const CURSOR: &str = "█";
