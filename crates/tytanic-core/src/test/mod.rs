//! Test loading and on-disk manipulation.

use std::fmt::Debug;
use std::time::{Duration, Instant};

use ecow::{eco_vec, EcoVec};
use typst::diag::SourceDiagnostic;

use crate::doc::{compare, compile};

mod annotation;
mod id;
pub mod template;
pub mod unit;

pub use self::annotation::{Annotation, ParseAnnotationError};
pub use self::id::{Id, ParseIdError};
pub use self::template::Test as TemplateTest;
pub use self::unit::Test as UnitTest;

/// A test within a test suite.
#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    /// A standalone unit test.
    Unit(UnitTest),

    /// A virtual designated template test.
    Template(TemplateTest),
}

impl Test {
    /// The unique id of this test.
    pub fn id(&self) -> &Id {
        match self {
            Test::Unit(test) => test.id(),
            Test::Template(test) => test.id(),
        }
    }

    /// Returns the inner unit test, or `None` if this is a template test.
    pub fn as_unit_test(&self) -> Option<&UnitTest> {
        match self {
            Test::Unit(test) => Some(test),
            Test::Template(_) => None,
        }
    }

    /// Returns the inner template test, or `None` if this is a unit test.
    pub fn as_template_test(&self) -> Option<&TemplateTest> {
        match self {
            Test::Unit(_) => None,
            Test::Template(test) => Some(test),
        }
    }
}

/// The stage of a single test run.
#[derive(Debug, Clone, Default)]
pub enum Stage {
    /// The test was cancelled or not started in the first place.
    #[default]
    Skipped,

    /// The test was filtered out by a [`Filter`].
    ///
    /// [`Filter`]: crate::suite::Filter
    Filtered,

    /// The test failed compilation.
    FailedCompilation {
        /// The inner error.
        error: compile::Error,

        /// Whether this was a compilation failure of the reference.
        reference: bool,
    },

    /// The test passed compilation, but failed comparison.
    FailedComparison(compare::Error),

    /// The test passed compilation, but did not run comparison.
    PassedCompilation,

    /// The test passed compilation and comparison.
    PassedComparison,

    /// The test passed compilation and updated its references.
    Updated {
        /// Whether the references were optimized.
        optimized: bool,
    },
}

/// The result of a single test run.
#[derive(Debug, Clone)]
pub struct TestResult {
    stage: Stage,
    warnings: EcoVec<SourceDiagnostic>,
    timestamp: Instant,
    duration: Duration,
}

impl TestResult {
    /// Create a result for a test for a skipped test. This will set the
    /// starting time to now, the duration to zero and the result to `None`.
    ///
    /// This can be used for constructing test results in advance to ensure an
    /// aborted test run contains a skip result for all yet-to-be-run tests.
    pub fn skipped() -> Self {
        Self {
            stage: Stage::Skipped,
            warnings: eco_vec![],
            timestamp: Instant::now(),
            duration: Duration::ZERO,
        }
    }

    /// Create a result for a test for a filtered test. This will set the
    /// starting time to now, the duration to zero and the result to filtered.
    pub fn filtered() -> Self {
        Self {
            stage: Stage::Filtered,
            warnings: eco_vec![],
            timestamp: Instant::now(),
            duration: Duration::ZERO,
        }
    }
}

impl TestResult {
    /// The stage of this rest result, if it was started.
    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    /// The warnings of the test emitted by the compiler.
    pub fn warnings(&self) -> &[SourceDiagnostic] {
        &self.warnings
    }

    /// The timestamp at which the suite run started.
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    /// The duration of the test, this a zero if this test wasn't started.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Whether the test was not started.
    pub fn is_skipped(&self) -> bool {
        matches!(&self.stage, Stage::Skipped)
    }

    /// Whether the test was filtered out.
    pub fn is_filtered(&self) -> bool {
        matches!(&self.stage, Stage::Filtered)
    }

    /// Whether the test passed compilation and/or comparison/update.
    pub fn is_pass(&self) -> bool {
        matches!(
            &self.stage,
            Stage::PassedCompilation | Stage::PassedComparison | Stage::Updated { .. }
        )
    }

    /// Whether the test failed compilation or comparison.
    pub fn is_fail(&self) -> bool {
        matches!(
            &self.stage,
            Stage::FailedCompilation { .. } | Stage::FailedComparison(..),
        )
    }

    /// The errors emitted by the compiler if compilation failed.
    pub fn errors(&self) -> Option<&[SourceDiagnostic]> {
        match &self.stage {
            Stage::FailedCompilation { error, .. } => Some(&error.0),
            _ => None,
        }
    }
}

impl TestResult {
    /// Sets the timestamp to [`Instant::now`].
    ///
    /// See [`TestResult::end`].
    pub fn start(&mut self) {
        self.timestamp = Instant::now();
    }

    /// Sets the duration to the time elapsed since [`TestResult::start`] was
    /// called.
    pub fn end(&mut self) {
        self.duration = self.timestamp.elapsed();
    }

    /// Sets the kind for this test to a compilation pass.
    pub fn set_passed_compilation(&mut self) {
        self.stage = Stage::PassedCompilation;
    }

    /// Sets the kind for this test to a reference compilation failure.
    pub fn set_failed_reference_compilation(&mut self, error: compile::Error) {
        self.stage = Stage::FailedCompilation {
            error,
            reference: true,
        };
    }

    /// Sets the kind for this test to a test compilation failure.
    pub fn set_failed_test_compilation(&mut self, error: compile::Error) {
        self.stage = Stage::FailedCompilation {
            error,
            reference: false,
        };
    }

    /// Sets the kind for this test to a test comparison pass.
    pub fn set_passed_comparison(&mut self) {
        self.stage = Stage::PassedComparison;
    }

    /// Sets the kind for this test to a comparison failure.
    pub fn set_failed_comparison(&mut self, error: compare::Error) {
        self.stage = Stage::FailedComparison(error);
    }

    /// Sets the kind for this test to a test update.
    pub fn set_updated(&mut self, optimized: bool) {
        self.stage = Stage::Updated { optimized };
    }

    /// Sets the warnings for this test.
    pub fn set_warnings<I>(&mut self, warnings: I)
    where
        I: Into<EcoVec<SourceDiagnostic>>,
    {
        self.warnings = warnings.into();
    }
}

impl Default for TestResult {
    fn default() -> Self {
        Self::skipped()
    }
}
