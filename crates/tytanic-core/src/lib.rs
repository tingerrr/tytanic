//! The core library of the Tytanic test runner.

pub mod config;
pub mod doc;
pub mod dsl;
pub mod library;
pub mod project;
pub mod suite;
pub mod test;
pub mod world_builder;

pub use project::Project;
pub use suite::FilteredSuite;
pub use suite::Suite;
pub use test::Id;
pub use test::TemplateTest;
pub use test::UnitTest;

/// The tool name, this is used in various places like config file directories,
/// manifest tool sections, and more.
pub const TOOL_NAME: &str = "tytanic";
