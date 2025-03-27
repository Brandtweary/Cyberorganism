//! GUI implementation using egui.
//! 
//! This module provides the GUI implementation for the cyberorganism task manager.
//! It replaces the previous TUI implementation while maintaining the same
//! minimalist interface design.

mod keyhandler;
mod rendering;
// Removed genius_feed module as it's been moved to archive

pub use rendering::run_app;
