// Theme data structures to be fully implemented in -03
// For now, provide minimal placeholder that Task 1-2 can reference

/// Represents a color theme
#[derive(Debug, Clone)]
pub struct Theme {
    pub id: String,
    pub name: String,
}

impl Theme {
    pub fn new(id: String, name: String) -> Self {
        Self { id, name }
    }
}
