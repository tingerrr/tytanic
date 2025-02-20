//! Loading and filtering of test suites, suites contain test and supplementary
//! fields for test templates, custom test set bindings and other information
//! necessary for managing, filtering and running tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};
use std::{fs, io};

use thiserror::Error;
use tytanic_filter::{eval, ExpressionFilter};
use tytanic_utils::fmt::Term;
use tytanic_utils::result::{io_not_found, ResultEx};
use uuid::Uuid;

use crate::project::Project;
use crate::test::{Id, LoadError, ParseIdError, Test, TestResult};

/// A suite of tests.
#[derive(Debug, Clone)]
pub struct Suite {
    tests: BTreeMap<Id, Test>,
    nested: BTreeMap<Id, Test>,
}

impl Suite {
    /// Creates a new empty suite.
    pub fn new() -> Self {
        Self {
            tests: BTreeMap::new(),
            nested: BTreeMap::new(),
        }
    }

    /// Recursively collects entries in the given directory.
    #[tracing::instrument(skip_all)]
    pub fn collect(project: &Project) -> Result<Self, Error> {
        let root = project.unit_tests_root();

        let mut this = Self::new();

        let Some(read_dir) = root.read_dir().ignore(io_not_found)? else {
            tracing::warn!(?root, "unit tests root not found");
            return Ok(this);
        };

        tracing::debug!("collecting from test root directory");
        for entry in read_dir {
            let entry = entry?;

            if entry.metadata()?.is_dir() {
                let abs = entry.path();
                let rel = abs
                    .strip_prefix(project.unit_tests_root())
                    .expect("entry must be in full");

                this.collect_dir(project, rel)?;
            }
        }

        let without_leafs: BTreeSet<_> = this
            .tests
            .keys()
            .flat_map(|test| test.ancestors().skip(1))
            .map(|test| test.to_owned())
            .collect();

        let all: BTreeSet<_> = this
            .tests
            .keys()
            .map(|test| test.as_str().to_owned())
            .collect();

        for id in all.intersection(&without_leafs) {
            if let Some((id, test)) = this.tests.remove_entry(id.as_str()) {
                this.nested.insert(id, test);
            }
        }

        if !this.nested.is_empty() {
            tracing::warn!("found {} nested tests", this.nested.len());
        }

        Ok(this)
    }

    /// Recursively collect tests in the given directory.
    fn collect_dir(&mut self, project: &Project, dir: &Path) -> Result<(), Error> {
        let abs = project.unit_tests_root().join(dir);

        let id = Id::new_from_path(dir)?;

        tracing::trace!(?dir, "checking for test");
        if let Some(test) = Test::load(project, id.clone())? {
            tracing::debug!(id = %test.id(), "collected test");
            self.tests.insert(id, test);
        }

        tracing::trace!(?dir, "collecting sub directories");
        for entry in fs::read_dir(&abs)? {
            let entry = entry?;

            if entry.metadata()?.is_dir() {
                let abs = entry.path();
                let rel = abs
                    .strip_prefix(project.unit_tests_root())
                    .expect("entry must be in full");

                self.collect_dir(project, rel)?;
            }
        }

        Ok(())
    }
}

impl Suite {
    /// The unit tests in this suite.
    pub fn tests(&self) -> &BTreeMap<Id, Test> {
        &self.tests
    }

    /// The nested tests, those which contain other tests and need to be migrated.
    ///
    /// This is a temporary method and will be removed in a future release.
    pub fn nested(&self) -> &BTreeMap<Id, Test> {
        &self.nested
    }
}

impl Suite {
    /// Apply a filter to a suite.
    pub fn filter(self, filter: Filter) -> Result<FilteredSuite, FilterError> {
        tracing::warn!(
            "ignoring {} nested tests while filtering",
            self.nested.len()
        );

        match &filter {
            Filter::TestSet(expr) => {
                let mut matched = BTreeMap::new();
                let mut filtered = BTreeMap::new();

                for (id, test) in &self.tests {
                    if expr.contains(test)? {
                        matched.insert(id.clone(), test.clone());
                    } else {
                        filtered.insert(id.clone(), test.clone());
                    }
                }

                Ok(FilteredSuite {
                    suite: self,
                    filter,
                    matched,
                    filtered,
                })
            }
            Filter::Explicit(set) => {
                let mut tests = self.tests.clone();
                let mut matched = BTreeMap::new();
                let mut missing = BTreeSet::new();

                for id in set {
                    match tests.remove_entry(id) {
                        Some((id, test)) => {
                            matched.insert(id, test);
                        }
                        None => {
                            missing.insert(id.clone());
                        }
                    }
                }

                if !missing.is_empty() {
                    return Err(FilterError::Missing(missing));
                }

                Ok(FilteredSuite {
                    suite: self,
                    filter,
                    matched,
                    filtered: tests,
                })
            }
        }
    }
}

impl Default for Suite {
    fn default() -> Self {
        Self::new()
    }
}

/// A filter used to restrict which tests in a suite should be run.
#[derive(Debug, Clone)]
pub enum Filter {
    /// A test set expression filter.
    TestSet(ExpressionFilter<Test>),

    /// An explicit set of test identifiers, if any of these cannot be found the
    /// filter fails.
    Explicit(BTreeSet<Id>),
}

/// A suite of tests with a filter applied to it.
#[derive(Debug, Clone)]
pub struct FilteredSuite {
    suite: Suite,
    filter: Filter,
    matched: BTreeMap<Id, Test>,
    filtered: BTreeMap<Id, Test>,
}

impl FilteredSuite {
    /// The unfiltered inner suite.
    pub fn suite(&self) -> &Suite {
        &self.suite
    }

    /// The filter that was used to filter the tests.
    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    /// The matched tests, i.e. those which _weren't_ filtered out.
    pub fn matched(&self) -> &BTreeMap<Id, Test> {
        &self.matched
    }

    /// The filtered tests, i.e. those which _were_ filtered out.
    pub fn filtered(&self) -> &BTreeMap<Id, Test> {
        &self.filtered
    }
}

/// Returned by [`Suite::filter`].
#[derive(Debug, Error)]
pub enum FilterError {
    /// An error occurred while evaluating an expresison filter.
    #[error("an error occurred while evaluating an expresison filter")]
    TestSet(#[from] eval::Error),

    /// At least one test given by an explicit filter was missing.
    #[error(
        "{} {} given by an explicit filter was missing",
        .0.len(),
        Term::simple("test").with(.0.len()),
    )]
    Missing(BTreeSet<Id>),
}

/// Returned by [`Suite::collect`].
#[derive(Debug, Error)]
pub enum Error {
    /// An error occurred while trying to parse a test [`Id`].
    #[error("an error occurred while collecting a test")]
    Id(#[from] ParseIdError),

    /// An error occurred while trying to collect a test.
    #[error("an error occurred while collecting a test")]
    Test(#[from] LoadError),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

/// The result of a test suite run, this contains results for all tests in a
/// suite, including filtered and not-yet-run tests, as well as cached values
/// for the number of filtered, passed and failed tests.
#[derive(Debug, Clone)]
pub struct SuiteResult {
    id: Uuid,
    total: usize,
    filtered: usize,
    passed: usize,
    failed: usize,
    timestamp: Instant,
    duration: Duration,
    results: BTreeMap<Id, TestResult>,
}

impl SuiteResult {
    /// Create a fresh result for a suite, this will have pre-filled results for
    /// all test set to cancelled, these results can be overridden while running
    /// the suite.
    pub fn new(suite: &FilteredSuite) -> Self {
        Self {
            id: Uuid::new_v4(),
            total: suite.suite().tests().len(),
            filtered: suite.filtered().len(),
            passed: 0,
            failed: 0,
            timestamp: Instant::now(),
            duration: Duration::ZERO,
            results: suite
                .matched()
                .keys()
                .map(|id| (id.clone(), TestResult::skipped()))
                .chain(
                    suite
                        .filtered()
                        .keys()
                        .map(|id| (id.clone(), TestResult::filtered())),
                )
                .collect(),
        }
    }
}

impl SuiteResult {
    /// The unique id of this run.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// The total number of tests in the suite, including filtered ones.
    pub fn total(&self) -> usize {
        self.total
    }

    /// The number of tests in the suite which were expected to run, i.e. the
    /// number of tests which were _not_ filtered out.
    pub fn expected(&self) -> usize {
        self.total - self.filtered
    }

    /// The number of tests in the suite which were run, regardless of outcome.
    pub fn run(&self) -> usize {
        self.passed + self.failed
    }

    /// The number of tests in the suite which were filtered out.
    pub fn filtered(&self) -> usize {
        self.filtered
    }

    /// The number of tests in the suite which were _not_ run due to
    /// cancellation.
    pub fn skipped(&self) -> usize {
        self.expected() - self.run()
    }

    /// The number of tests in the suite which passed.
    pub fn passed(&self) -> usize {
        self.passed
    }

    /// The number of tests in the suite which failed.
    pub fn failed(&self) -> usize {
        self.failed
    }

    /// The timestamp at which the suite run started.
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    /// The duration of the whole suite run.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// The individual test results.
    ///
    /// This contains results for all tests in the a suite, not just those added
    /// in [`SuiteResult::set_test_result`].
    pub fn results(&self) -> &BTreeMap<Id, TestResult> {
        &self.results
    }

    /// Whether this suite can be considered a complete pass.
    pub fn is_complete_pass(&self) -> bool {
        self.expected() == self.passed()
    }
}

impl SuiteResult {
    /// Sets the timestamp to [`Instant::now`].
    ///
    /// See [`SuiteResult::end`].
    pub fn start(&mut self) {
        self.timestamp = Instant::now();
    }

    /// Sets the duration to the time elapsed since [`SuiteResult::start`] was
    /// called.
    pub fn end(&mut self) {
        self.duration = self.timestamp.elapsed();
    }

    /// Add a test result.
    ///
    /// - This should only add results for each test once, otherwise the test
    ///   will be counted multiple times.
    /// - The results should also only contain failures or passes, cancellations
    ///   and filtered results are ignored, as these are pre-filled when the
    ///   result is constructed.
    pub fn set_test_result(&mut self, id: Id, result: TestResult) {
        debug_assert!(self.results.contains_key(&id));
        debug_assert!(result.is_pass() || result.is_fail());

        if result.is_pass() {
            self.passed += 1;
        } else {
            self.failed += 1;
        }

        self.results.insert(id, result);
    }
}

#[cfg(test)]
mod tests {
    use ecow::eco_vec;
    use tytanic_utils::fs::TempTestEnv;

    use super::*;
    use crate::test::{Annotation, Kind};

    #[test]
    fn test_collect() {
        TempTestEnv::run_no_check(
            |root| {
                root
                    // compile only
                    .setup_file("tests/compile-only/test.typ", "Hello World")
                    // regular ephemeral
                    .setup_file("tests/compare/ephemeral/test.typ", "Hello World")
                    .setup_file("tests/compare/ephemeral/ref.typ", "Hello\nWorld")
                    // ephemeral despite ref directory
                    .setup_file("tests/compare/ephemeral-store/test.typ", "Hello World")
                    .setup_file("tests/compare/ephemeral-store/ref.typ", "Hello\nWorld")
                    .setup_file("tests/compare/ephemeral-store/ref", "Blah Blah")
                    // persistent
                    .setup_file("tests/compare/persistent/test.typ", "Hello World")
                    .setup_file("tests/compare/persistent/ref", "Blah Blah")
                    // not a test
                    .setup_file_empty("tests/not-a-test/test.txt")
                    // ignored test
                    .setup_file("tests/ignored/test.typ", "/// [skip]\nHello World")
            },
            |root| {
                let project = Project::new(root);
                let suite = Suite::collect(&project).unwrap();

                let tests = [
                    ("compile-only", Kind::CompileOnly, eco_vec![]),
                    ("compare/ephemeral", Kind::Ephemeral, eco_vec![]),
                    ("compare/ephemeral-store", Kind::Ephemeral, eco_vec![]),
                    ("compare/persistent", Kind::Persistent, eco_vec![]),
                    ("ignored", Kind::CompileOnly, eco_vec![Annotation::Skip]),
                ];

                for (key, kind, annotations) in tests {
                    let test = &suite.tests[key];
                    assert_eq!(test.annotations(), &annotations[..]);
                    assert_eq!(test.kind(), kind);
                }
            },
        );
    }
}
