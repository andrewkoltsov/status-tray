//! Pure logic for Status Tray. No Tauri dependencies.

pub mod catalog;
pub mod diff;
pub mod error;
pub mod model;
pub mod notify;
pub mod provider;
pub mod update;

pub use error::{Error, Result};
