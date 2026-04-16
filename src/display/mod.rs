// SPDX-License-Identifier: MIT

use crate::departure::model::BoardPayload;

/// Trait for rendering departure data to any output target.
/// Implement this to add new display backends (web, Pixoo64, etc.).
pub trait DisplayRenderer: Send + Sync + 'static {
    /// Called when new departure data is available.
    /// `payload.boards` contains one entry per monitored stop.
    /// Pixoo64 should use `boards[0]`; the web renderer broadcasts all boards.
    fn update(&self, payload: &BoardPayload) -> Result<(), Box<dyn std::error::Error>>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}
