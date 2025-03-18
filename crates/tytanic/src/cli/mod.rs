use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::{env, io};

use color_eyre::eyre;
use color_eyre::eyre::WrapErr;
use commands::CompileOptions;
use termcolor::Color;
use thiserror::Error;
use tytanic_core::project::{ConfigError, ManifestError, Project, ShallowProject};
use tytanic_core::suite::{Filter, FilterError, FilteredSuite, Suite};
use tytanic_core::test::{self, ParseIdError};
use tytanic_core::{doc, dsl};
use tytanic_filter::{eval, ExpressionFilter};

use self::commands::{CliArguments, FilterOptions, Switch};
use crate::ui::{self, Ui};
use crate::world::SystemWorld;
use crate::{cwrite, kit};

pub mod commands;

/// Whether we received a signal we can gracefully exit from.
pub static CANCELLED: AtomicBool = AtomicBool::new(false);

/// Tytanic exited successfully.
pub const EXIT_OK: u8 = 0;

/// At least one test failed.
pub const EXIT_TEST_FAILURE: u8 = 1;

/// The requested operation failed gracefully.
pub const EXIT_OPERATION_FAILURE: u8 = 2;

/// An unexpected error occurred.
pub const EXIT_ERROR: u8 = 3;

/// A graceful error.
#[derive(Debug, Error)]
#[error("an operation failed")]
pub struct OperationFailure;

/// A test failure.
#[derive(Debug, Error)]
#[error("one or more test failed")]
pub struct TestFailure;

pub struct Context<'a> {
    /// The parsed top-level arguments.
    pub args: &'a CliArguments,

    /// The terminal ui.
    pub ui: &'a Ui,
}

impl<'a> Context<'a> {
    pub fn new(args: &'a CliArguments, ui: &'a Ui) -> Self {
        Self { args, ui }
    }
}

impl Context<'_> {
    /// Emit an error that the given expression evaluated to more than the
    /// allowed number of tests for some operation.
    pub fn error_too_many_tests(&self, expr: &str) -> io::Result<()> {
        writeln!(self.ui.error()?, "Matched more than one test")?;

        let mut w = self.ui.hint()?;
        write!(w, "use '")?;
        cwrite!(colored(w, Color::Cyan), "all:")?;
        writeln!(w, "{expr}' to confirm using all tests")
    }
}

// TODO(tinger): cache these values
impl Context<'_> {
    /// Resolve the current root.
    #[tracing::instrument(skip_all)]
    pub fn root(&self) -> eyre::Result<PathBuf> {
        Ok(match &self.args.root {
            Some(root) => {
                if !root.try_exists()? {
                    writeln!(self.ui.error()?, "Root '{}' not found", root.display())?;
                    eyre::bail!(OperationFailure);
                }

                root.canonicalize()?
            }
            None => env::current_dir().wrap_err("reading PWD")?,
        })
    }

    /// Discover the current and ensure it is initialized.
    #[tracing::instrument(skip_all)]
    pub fn project(&self) -> eyre::Result<Project> {
        let root = self.root()?;

        let Some(project) = ShallowProject::discover(root, self.args.root.is_some())? else {
            writeln!(self.ui.error()?, "Must be in a typst project")?;

            let mut w = self.ui.hint()?;
            write!(w, "You can pass the project root using ")?;
            cwrite!(colored(w, Color::Cyan), "--root <path>")?;
            writeln!(w)?;
            eyre::bail!(OperationFailure);
        };

        Ok(project.load()?)
    }

    /// Create a new filter from given arguments.
    #[tracing::instrument(skip_all)]
    pub fn filter(&self, filter: &FilterOptions) -> eyre::Result<Filter> {
        if !filter.tests.is_empty() {
            Ok(Filter::Explicit(filter.tests.iter().cloned().collect()))
        } else {
            let ctx = dsl::context();
            let mut set = ExpressionFilter::new(ctx, &filter.expression)?;

            if filter.skip.get_or_default() {
                set = set.map(|set| eval::Set::expr_diff(set, dsl::built_in::skip()));
            }

            Ok(Filter::TestSet(set))
        }
    }

    /// Collect and filter tests for the given project.
    #[tracing::instrument(skip_all)]
    pub fn collect_tests_with_filter(
        &self,
        project: &Project,
        filter: Filter,
    ) -> eyre::Result<FilteredSuite> {
        let suite = self.collect_tests(project)?;

        if suite.is_empty() {
            writeln!(self.ui.warn()?, "Suite is empty")?;
        }

        let suite = suite.filter(filter)?;

        if suite.matched().is_empty() {
            writeln!(self.ui.warn()?, "Test set matched no tests")?;
        }

        Ok(suite)
    }

    /// Collect all tests for the given project.
    #[tracing::instrument(skip_all)]
    pub fn collect_tests(&self, project: &Project) -> eyre::Result<Suite> {
        let suite = Suite::collect(project)?;

        if !suite.nested().is_empty() {
            writeln!(self.ui.warn()?, "Found nested tests")?;

            writeln!(
                self.ui.hint()?,
                "This is no longer supported, these tests will be ignored"
            )?;
            writeln!(
                self.ui.hint()?,
                "This will become a hard error in a future version"
            )?;

            let mut w = self.ui.hint()?;
            write!(w, "You can run ")?;
            cwrite!(colored(w, Color::Cyan), "tt util migrate")?;
            writeln!(w, " to automatically move the tests")?;
        }

        Ok(suite)
    }

    /// Create a SystemWorld from the given args.
    #[tracing::instrument(skip_all)]
    pub fn world(&self, compile_options: &CompileOptions) -> eyre::Result<SystemWorld> {
        kit::world(
            self.root()?,
            &self.args.font,
            &self.args.package,
            compile_options,
        )
    }
}

impl Context<'_> {
    /// Run the parsed command and report errors as ui messages.
    #[tracing::instrument(skip_all)]
    pub fn run(&mut self) -> eyre::Result<()> {
        let Err(error) = self.args.cmd.run(self) else {
            return Ok(());
        };

        for error in error.chain() {
            // TODO(tinger): attach test id
            if let Some(doc::LoadError::MissingPages(pages)) = error.downcast_ref() {
                if pages.is_empty() {
                    writeln!(self.ui.error()?, "References had zero pages")?;
                } else {
                    writeln!(
                        self.ui.error()?,
                        "References had missing pages, these pages were found: {pages:?}"
                    )?;
                }

                eyre::bail!(OperationFailure);
            }

            // TODO(tinger): attach test id
            if let Some(error) = error.downcast_ref::<ParseIdError>() {
                match error {
                    ParseIdError::InvalidFragment => {
                        writeln!(self.ui.error()?, "A test identifier must not contain other characters than non-alphanumeric, hyphens and underscores")?;
                    }
                    ParseIdError::Empty => {
                        writeln!(self.ui.error()?, "A test identifier must not be empty")?;
                    }
                }

                eyre::bail!(OperationFailure);
            }

            // TODO(tinger): attach test id
            if let Some(error) = error.downcast_ref::<test::ParseAnnotationError>() {
                writeln!(self.ui.error()?, "Couldn't parse annotations:\n{error}")?;
                eyre::bail!(OperationFailure);
            }

            if let Some(error) = error.downcast_ref::<ManifestError>() {
                match error {
                    ManifestError::Parse(error) => {
                        writeln!(self.ui.error()?, "Failed to parse manifest:\n{error}")?;
                        eyre::bail!(OperationFailure);
                    }
                    ManifestError::Invalid(error) => {
                        writeln!(self.ui.error()?, "Failed to validate manifest:\n{error}")?;
                        eyre::bail!(OperationFailure);
                    }
                    _ => {}
                }
            }

            if let Some(error) = error.downcast_ref::<ConfigError>() {
                match error {
                    ConfigError::Parse(error) => {
                        writeln!(self.ui.error()?, "Failed to parse config:\n{error}")?;
                        eyre::bail!(OperationFailure);
                    }
                    ConfigError::Invalid(error) => {
                        writeln!(self.ui.error()?, "Failed to validate config:\n{error}")?;
                        eyre::bail!(OperationFailure);
                    }
                    _ => {}
                }
            }

            if let Some(error) = error.downcast_ref::<tytanic_filter::Error>() {
                match error {
                    tytanic_filter::Error::Parse(error) => {
                        writeln!(self.ui.error()?, "Couldn't parse test set:\n{error}")?;
                    }
                    tytanic_filter::Error::Eval(error) => {
                        writeln!(self.ui.error()?, "Couldn't evaluate test set:\n{error}")?;
                    }
                }

                eyre::bail!(OperationFailure);
            }

            if let Some(error) = error.downcast_ref::<FilterError>() {
                match error {
                    FilterError::TestSet(error) => {
                        writeln!(self.ui.error()?, "Couldn't evaluate test set:\n{error}")?;
                    }
                    FilterError::Missing(missing) => {
                        let mut w = self.ui.error()?;

                        for id in missing {
                            write!(w, "Test ")?;
                            ui::write_test_id(&mut w, id)?;
                            writeln!(w, " not found")?;
                        }
                    }
                }

                eyre::bail!(OperationFailure);
            }
        }

        eyre::bail!(error);
    }
}
