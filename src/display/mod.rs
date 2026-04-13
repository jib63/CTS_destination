// SPDX-License-Identifier: MIT

use crate::departure::model::DepartureBoard;

/// Trait for rendering departure data to any output target.
/// Implement this to add new display backends (web, Pixoo64, etc.).
pub trait DisplayRenderer: Send + Sync + 'static {
    /// Called when new departure data is available.
    fn update(&self, board: &DepartureBoard) -> Result<(), Box<dyn std::error::Error>>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}
