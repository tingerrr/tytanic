//! The core library of the tytanic test runner.

pub mod config;
pub mod doc;
pub mod dsl;
pub mod library;
pub mod project;
pub mod suite;
pub mod test;

pub use project::Project;
pub use suite::{FilteredSuite, Suite};
pub use test::{Id, TemplateTest, UnitTest};

#[cfg(test)]
mod _dev;

/// The tool name, this is used in various places like config file directories,
/// manifest tool sections, and more.
pub const TOOL_NAME: &str = "tytanic";
