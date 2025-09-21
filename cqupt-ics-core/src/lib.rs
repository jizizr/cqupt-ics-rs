//! CQUPT ICS Core Library
//!
//! This library provides core functionality for generating ICS calendar files
//! from CQUPT course data.

pub mod cache;
pub mod error;
pub mod ics;
pub mod location;
pub mod providers;
pub mod types;

// Re-export core types and error handling
pub use error::{Error, Result};
pub use types::*;

/// Commonly used items
pub mod prelude {
    pub use crate::{cache::*, ics::*, location::*, providers::*, types::*};
}
