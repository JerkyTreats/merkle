//! CLI output: error mapping from domain errors to stable CLI surface.

use crate::error::ApiError;

/// Map domain/service errors to a string for CLI output.
/// Keeps route handlers thin; extend with stable categories if needed.
pub fn map_error(e: &ApiError) -> String {
    e.to_string()
}
