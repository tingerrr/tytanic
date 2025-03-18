use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::eyre::{self, ContextCompat, WrapErr};
use typst::diag::Warned;
use typst::layout::PagedDocument;
use typst::syntax::Source;
use tytanic_core::config::Direction;
use tytanic_core::doc::compare::Strategy;
use tytanic_core::doc::compile::Warnings;
use tytanic_core::doc::render::{self, Origin};
use tytanic_core::doc::{compile, Document};
use tytanic_core::project::Project;
use tytanic_core::suite::{FilteredSuite, SuiteResult};
use tytanic_core::test::unit::Kind;
use tytanic_core::test::{Annotation, Test, TestResult};
use tytanic_core::{TemplateTest, UnitTest};

use crate::cli::TestFailure;
use crate::report::Reporter;
use crate::world::SystemWorld;
use crate::DEFAULT_OPTIMIZE_OPTIONS;

#[derive(Debug, Clone)]
pub enum Action {
    /// Compile and optionally compare tests.
    Run {
        /// The strategy to use when comparing documents.
        strategy: Option<Strategy>,

        /// Whether to export ephemeral output.
        export_ephemeral: bool,

        /// The origin at which to render diff images of different dimensions.
        origin: Origin,
    },

    /// Compile and update test references.
    Update {
        /// Whether to export ephemeral output.
        export_ephemeral: bool,

        /// The origin at which to render diff images of different dimensions.
        origin: Origin,
    },
}

#[derive(Debug, Clone)]
pub struct RunnerConfig<'c> {
    /// How to handle warnings.
    pub warnings: Warnings,

    /// Whether to optimize reference documents.
    pub optimize: bool,

    /// Whether to stop after the first failure.
    pub fail_fast: bool,

    /// The pixel-per-pt to use when rendering documents.
    pub pixel_per_pt: f32,

    /// The action to take for the test.
    pub action: Action,

    /// A cancellation flag used to abort a test run.
    pub cancellation: &'c AtomicBool,
}

pub struct Runner<'c, 'p> {
    pub project: &'p Project,
    pub suite: &'p FilteredSuite,
    pub world: &'p SystemWorld,

    pub result: SuiteResult,
    pub config: RunnerConfig<'c>,
}

impl<'c, 'p> Runner<'c, 'p> {
    pub fn new(
        project: &'p Project,
        suite: &'p FilteredSuite,
        world: &'p SystemWorld,
        config: RunnerConfig<'c>,
    ) -> Self {
        Self {
            project,
            result: SuiteResult::new(suite),
            suite,
            world,
            config,
        }
    }

    pub fn unit_test<'s>(&'s self, test: &'p UnitTest) -> UnitTestRunner<'c, 's, 'p> {
        UnitTestRunner {
            project_runner: self,
            test,
            result: TestResult::skipped(),
        }
    }

    pub fn template_test<'s>(&'s self, test: &'p TemplateTest) -> TemplateTestRunner<'c, 's, 'p> {
        TemplateTestRunner {
            project_runner: self,
            test,
            result: TestResult::skipped(),
        }
    }

    pub fn run_inner(&mut self, reporter: &Reporter) -> eyre::Result<()> {
        reporter.report_status(&self.result)?;

        for test in self.suite.matched() {
            if self.config.cancellation.load(Ordering::SeqCst) {
                return Ok(());
            }

            let result = match test {
                Test::Unit(test) => self.unit_test(test).run()?,
                Test::Template(test) => self.template_test(test).run()?,
            };

            reporter.clear_status()?;

            // TODO(tinger): retrieve export var from action
            reporter.report_test_result(test, &result)?;

            if result.is_fail() && self.config.fail_fast {
                self.result.set_test_result(test.id().clone(), result);
                return Ok(());
            }

            reporter.report_status(&self.result)?;

            self.result.set_test_result(test.id().clone(), result);
        }

        reporter.clear_status()?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub fn run(mut self, reporter: &Reporter) -> eyre::Result<SuiteResult> {
        self.result.start();
        reporter.report_start(&self.result)?;
        let res = self.run_inner(reporter);
        self.result.end();
        reporter.report_end(&self.result)?;

        res?;

        Ok(self.result)
    }
}

pub struct UnitTestRunner<'c, 's, 'p> {
    project_runner: &'s Runner<'c, 'p>,
    test: &'p UnitTest,
    result: TestResult,
}

impl UnitTestRunner<'_, '_, '_> {
    fn run_inner(&mut self) -> eyre::Result<()> {
        // TODO(tinger): don't exit early if there are still exports possible

        match self.project_runner.config.action {
            Action::Run {
                strategy,
                export_ephemeral: export,
                origin,
            } => {
                let output = self.load_out_src()?;
                let output = self.compile_out_doc(output)?;
                let output = self.render_out_doc(output)?;

                if export {
                    self.export_out_doc(&output)?;
                }

                match self.test.kind() {
                    Kind::Ephemeral => {
                        let reference = self.load_ref_src()?;
                        let reference = self.compile_ref_doc(reference)?;
                        let reference = self.render_ref_doc(reference)?;

                        if export {
                            self.export_ref_doc(&reference)?;

                            let diff = self.render_diff_doc(&output, &reference, origin)?;
                            self.export_diff_doc(&diff)?;
                        }

                        if let Some(strategy) = strategy {
                            if let Err(err) = self.compare(&output, &reference, strategy) {
                                eyre::bail!(err);
                            }
                        }
                    }
                    Kind::Persistent => {
                        let reference = self.load_ref_doc()?;

                        // TODO(tinger): don't unconditionally export this
                        // perhaps? on the other hand without comparison we
                        // don't know whether this is meaningful or not
                        if export {
                            let diff = self.render_diff_doc(&output, &reference, origin)?;
                            self.export_diff_doc(&diff)?;
                        }

                        if let Some(strategy) = strategy {
                            if let Err(err) = self.compare(&output, &reference, strategy) {
                                eyre::bail!(err);
                            }
                        }
                    }
                    Kind::CompileOnly => {}
                }
            }
            Action::Update {
                export_ephemeral: export,
                origin,
            } => match self.test.kind() {
                Kind::Ephemeral => {
                    let output = self.load_out_src()?;
                    let output = self.compile_out_doc(output)?;
                    let output = self.render_out_doc(output)?;

                    if export {
                        self.export_out_doc(&output)?;
                    }
                }
                Kind::Persistent => {
                    let output = self.load_out_src()?;
                    let output = self.compile_out_doc(output)?;
                    let output = self.render_out_doc(output)?;

                    self.test.create_reference_document(
                        self.project_runner.project,
                        &output,
                        self.project_runner
                            .config
                            .optimize
                            .then_some(&*DEFAULT_OPTIMIZE_OPTIONS),
                    )?;

                    self.result.set_updated(self.project_runner.config.optimize);

                    if export {
                        let reference = self.load_ref_doc()?;
                        self.export_out_doc(&reference)?;

                        let diff = self.render_diff_doc(&output, &reference, origin)?;
                        self.export_diff_doc(&diff)?;
                    }
                }
                Kind::CompileOnly => eyre::bail!("attempted to update compile-only test"),
            },
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub fn run(mut self) -> eyre::Result<TestResult> {
        self.result.start();
        self.prepare()?;
        let res = self.run_inner();
        self.cleanup()?;
        self.result.end();

        if let Err(err) = res {
            if !err.chain().any(|s| s.is::<TestFailure>()) {
                eyre::bail!(err);
            }
        }

        Ok(self.result)
    }

    pub fn prepare(&mut self) -> eyre::Result<()> {
        tracing::trace!(test = ?self.test.id(), "clearing temporary directories");

        self.test
            .create_temporary_directories(self.project_runner.project)?;

        Ok(())
    }

    pub fn cleanup(&mut self) -> eyre::Result<()> {
        Ok(())
    }

    pub fn load_out_src(&mut self) -> eyre::Result<Source> {
        tracing::trace!(test = ?self.test.id(), "loading output source");
        Ok(self.test.load_source(self.project_runner.project)?)
    }

    pub fn load_ref_src(&mut self) -> eyre::Result<Source> {
        tracing::trace!(test = ?self.test.id(), "loading reference source");

        if !self.test.kind().is_ephemeral() {
            eyre::bail!("attempted to load reference source for non-ephemeral test");
        }

        self.test
            .load_reference_source(self.project_runner.project)?
            .wrap_err_with(|| format!("couldn't load reference source for test {}", self.test.id()))
    }

    pub fn load_ref_doc(&mut self) -> eyre::Result<Document> {
        tracing::trace!(test = ?self.test.id(), "loading reference document");

        if !self.test.kind().is_persistent() {
            eyre::bail!("attempted to load reference source for non-persistent test");
        }

        self.test
            .load_reference_document(self.project_runner.project)
            .wrap_err_with(|| {
                format!(
                    "couldn't load reference document for test {}",
                    self.test.id()
                )
            })
    }

    pub fn render_out_doc(&mut self, doc: PagedDocument) -> eyre::Result<Document> {
        tracing::trace!(test = ?self.test.id(), "rendering output document");

        let mut pixel_per_pt = self.project_runner.config.pixel_per_pt;
        for annot in self.test.annotations().iter() {
            if let Annotation::Ppi(ppi) = annot {
                pixel_per_pt = render::ppi_to_ppp(*ppi)
            }
        }

        Ok(Document::render(doc, pixel_per_pt))
    }

    pub fn render_ref_doc(&mut self, doc: PagedDocument) -> eyre::Result<Document> {
        tracing::trace!(test = ?self.test.id(), "rendering reference document");

        if !self.test.kind().is_ephemeral() {
            eyre::bail!("attempted to render reference for non-ephemeral test");
        }

        let mut pixel_per_pt = self.project_runner.config.pixel_per_pt;
        for annot in self.test.annotations().iter() {
            if let Annotation::Ppi(ppi) = annot {
                pixel_per_pt = render::ppi_to_ppp(*ppi)
            }
        }

        Ok(Document::render(doc, pixel_per_pt))
    }

    pub fn render_diff_doc(
        &mut self,
        output: &Document,
        reference: &Document,
        mut origin: Origin,
    ) -> eyre::Result<Document> {
        tracing::trace!(test = ?self.test.id(), "rendering difference document");

        if self.test.kind().is_compile_only() {
            eyre::bail!("attempted to render difference document for compile-only test");
        }

        for annot in self.test.annotations().iter() {
            match annot {
                Annotation::Dir(Direction::Ltr) => origin = Origin::TopLeft,
                Annotation::Dir(Direction::Rtl) => origin = Origin::TopRight,
                _ => {}
            }
        }

        Ok(Document::render_diff(reference, output, origin))
    }

    pub fn compile_out_doc(&mut self, output: Source) -> eyre::Result<PagedDocument> {
        tracing::trace!(test = ?self.test.id(), "compiling output document");

        self.compile_inner(output, false)
    }

    pub fn compile_ref_doc(&mut self, reference: Source) -> eyre::Result<PagedDocument> {
        tracing::trace!(test = ?self.test.id(), "compiling reference document");

        if self.test.kind().is_compile_only() {
            eyre::bail!("attempted to compile reference for compile-only test");
        }

        self.compile_inner(reference, true)
    }

    fn compile_inner(&mut self, source: Source, is_reference: bool) -> eyre::Result<PagedDocument> {
        let Warned { output, warnings } = compile::compile(
            source,
            self.project_runner.world,
            self.project_runner.config.warnings,
            // NOTE(tinger): We only use augmentation here because package
            // rerouting should not happen for unit tests.
            |w| w.augment_standard_library(true),
        );

        self.result.set_warnings(warnings);

        let doc = match output {
            Ok(doc) => {
                self.result.set_passed_compilation();
                doc
            }
            Err(err) => {
                if is_reference {
                    self.result.set_failed_reference_compilation(err);
                } else {
                    self.result.set_failed_test_compilation(err);
                }
                eyre::bail!(TestFailure);
            }
        };

        Ok(doc)
    }

    pub fn export_ref_doc(&mut self, reference: &Document) -> eyre::Result<()> {
        tracing::trace!(test = ?self.test.id(), "saving reference document");

        if !self.test.kind().is_ephemeral() {
            eyre::bail!("attempted to save reference document for non-ephemeral test");
        }

        reference.save(
            self.project_runner
                .project
                .unit_test_ref_dir(self.test.id()),
            None,
        )?;

        Ok(())
    }

    pub fn export_out_doc(&mut self, output: &Document) -> eyre::Result<()> {
        tracing::trace!(test = ?self.test.id(), "saving output document");

        output.save(
            self.project_runner
                .project
                .unit_test_out_dir(self.test.id()),
            None,
        )?;

        Ok(())
    }

    pub fn export_diff_doc(&mut self, doc: &Document) -> eyre::Result<()> {
        tracing::trace!(test = ?self.test.id(), "saving difference document");

        if self.test.kind().is_compile_only() {
            eyre::bail!("attempted to save difference document for compile-only test");
        }

        doc.save(
            self.project_runner
                .project
                .unit_test_diff_dir(self.test.id()),
            None,
        )?;

        Ok(())
    }

    pub fn compare(
        &mut self,
        output: &Document,
        reference: &Document,
        strategy: Strategy,
    ) -> eyre::Result<()> {
        tracing::trace!(test = ?self.test.id(), "comparing");

        if self.test.kind().is_compile_only() {
            eyre::bail!("attempted to compare compile-only test");
        }

        let Strategy::Simple {
            mut max_delta,
            mut max_deviation,
        } = strategy;

        for annot in self.test.annotations().iter() {
            match annot {
                Annotation::MaxDelta(set) => max_delta = *set,
                Annotation::MaxDeviations(set) => max_deviation = *set,
                _ => {}
            }
        }

        if let Err(error) = Document::compare(
            output,
            reference,
            Strategy::Simple {
                max_delta,
                max_deviation,
            },
        ) {
            self.result.set_failed_comparison(error);
            eyre::bail!(TestFailure);
        }

        self.result.set_passed_comparison();

        Ok(())
    }
}

pub struct TemplateTestRunner<'c, 's, 'p> {
    project_runner: &'s Runner<'c, 'p>,
    test: &'p TemplateTest,
    result: TestResult,
}

impl TemplateTestRunner<'_, '_, '_> {
    // TODO: suite, different world root and lookup behavior
    fn run_inner(&mut self) -> eyre::Result<()> {
        match self.project_runner.config.action {
            Action::Run {
                // export_ephemeral: export,
                ..
            } => {
                let output = self.load_template_src()?;
                let _output = self.compile_template(output)?;

                // if export {
                //     let output = self.render_template_doc(output)?;
                //     self.export_out_doc(&output)?;
                // }
            }
            Action::Update { .. } => eyre::bail!("attempted to update template test"),
        }

        Ok(())
    }

    pub fn run(mut self) -> eyre::Result<TestResult> {
        self.result.start();
        self.prepare()?;
        let res = self.run_inner();
        self.cleanup()?;
        self.result.end();

        if let Err(err) = res {
            if !err.chain().any(|s| s.is::<TestFailure>()) {
                eyre::bail!(err);
            }
        }

        Ok(self.result)
    }

    pub fn prepare(&mut self) -> eyre::Result<()> {
        Ok(())
    }

    pub fn cleanup(&mut self) -> eyre::Result<()> {
        Ok(())
    }

    pub fn load_template_src(&mut self) -> eyre::Result<Source> {
        tracing::trace!(test = ?self.test.id(), "loading template source");
        Ok(self.test.load_source(self.project_runner.project)?)
    }

    pub fn compile_template(&mut self, source: Source) -> eyre::Result<PagedDocument> {
        let Warned { output, warnings } = compile::compile(
            source,
            self.project_runner.world,
            self.project_runner.config.warnings,
            |w| {
                w.reroute_package(self.project_runner.project.package_spec())
                    .root_prefix(
                        self.project_runner
                            .project
                            .manifest()
                            .and_then(|m| m.template.as_ref())
                            .map(|t| t.path.as_str().into()),
                    )
            },
        );

        self.result.set_warnings(warnings);

        let doc = match output {
            Ok(doc) => {
                self.result.set_passed_compilation();
                doc
            }
            Err(err) => {
                self.result.set_failed_test_compilation(err);
                eyre::bail!(TestFailure);
            }
        };

        Ok(doc)
    }
}
