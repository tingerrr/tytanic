//! Live reporting of test progress.

use std::io;
use std::io::Write;

use chrono::TimeDelta;
use chrono::Utc;
use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::doc::compare;
use tytanic_core::doc::compare::PageError;
use tytanic_core::suite::SuiteResult;
use tytanic_core::test::Stage;
use tytanic_core::test::Test;
use tytanic_core::test::TestResult;
use tytanic_utils::fmt::Term;

use crate::cwrite;
use crate::ui;
use crate::ui::CWrite;
use crate::ui::Ui;
use crate::world::SystemWorld;

/// The padding to use for annotations while test run reporting.
const RUN_ANNOT_PADDING: usize = 10;

/// A reporter for test output and test run status reporting.
pub struct Reporter<'ui, 'p> {
    ui: &'ui Ui,
    world: &'p SystemWorld,

    live: bool,
}

impl<'ui, 'p> Reporter<'ui, 'p> {
    pub fn new(ui: &'ui Ui, world: &'p SystemWorld, live: bool) -> Self {
        Self { ui, world, live }
    }
}

impl Reporter<'_, '_> {
    /// Reports the start of a test run.
    pub fn report_start(&self, result: &SuiteResult) -> io::Result<()> {
        let mut w = ui::annotated(
            self.ui.stderr(),
            "Starting",
            Color::Green,
            RUN_ANNOT_PADDING,
        )?;

        cwrite!(bold(w), "{}", result.total())?;
        write!(w, " tests")?;

        if result.filtered() != 0 {
            write!(w, ", ")?;
            cwrite!(bold(w), "{}", result.filtered())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Yellow), "filtered")?;
        }

        write!(w, " (run ID: ")?;
        cwrite!(bold(w), "{}", result.id())?;
        writeln!(w, ")")?;

        Ok(())
    }

    /// Reports the end of a test run.
    pub fn report_end(&self, result: &SuiteResult) -> io::Result<()> {
        let mut w = self.ui.stderr();

        let color = if result.failed() == 0 {
            Color::Green
        } else if result.passed() == 0 {
            Color::Red
        } else {
            Color::Yellow
        };

        writeln!(w, "{:─>RUN_ANNOT_PADDING$}", "")?;

        let mut w = ui::annotated(w, "Summary", color, RUN_ANNOT_PADDING)?;

        write!(w, "[")?;
        {
            let mut w = ui::colored(
                &mut w,
                duration_color(
                    result
                        .duration()
                        .checked_div(result.run() as i32)
                        .unwrap_or_default(),
                ),
            )?;
            write_duration(&mut w, result.duration())?;
            w.finish()?;
        }
        write!(w, "] ")?;

        cwrite!(bold(w), "{}", result.run())?;
        write!(w, "/")?;
        cwrite!(bold(w), "{}", result.expected())?;
        write!(w, " tests run: ")?;

        if result.passed() == result.total() {
            cwrite!(bold(w), "all {}", result.passed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Green), "passed")?;
        } else if result.failed() == result.total() {
            cwrite!(bold(w), "all {}", result.failed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Red), "failed")?;
        } else {
            cwrite!(bold(w), "{}", result.passed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Green), "passed")?;

            write!(w, ", ")?;
            cwrite!(bold(w), "{}", result.failed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Red), "failed")?;
        }

        if result.filtered() != 0 {
            write!(w, ", ")?;
            cwrite!(bold(w), "{}", result.filtered())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Yellow), "filtered")?;
        }

        if result.skipped() != 0 {
            write!(w, ", ")?;
            cwrite!(bold(w), "{}", result.skipped())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Yellow), "skipped")?;
        }

        writeln!(w)?;

        // TODO(tinger): Report failures, mean, and average time.

        Ok(())
    }

    /// Clears the last line, i.e the status output.
    pub fn clear_status(&self) -> io::Result<()> {
        if !self.live {
            return Ok(());
        }

        write!(self.ui.stderr(), "\x1B[0F\x1B[0J")
    }

    /// Reports the current status of an ongoing test run.
    pub fn report_status(&self, result: &SuiteResult) -> io::Result<()> {
        if !self.live {
            return Ok(());
        }

        let duration = Utc::now().signed_duration_since(result.timestamp());

        let mut w = ui::annotated(self.ui.stderr(), "", Color::Black, RUN_ANNOT_PADDING)?;

        write!(w, "[")?;
        {
            let mut w = ui::colored(
                &mut w,
                duration_color(
                    duration
                        .checked_div(result.run() as i32)
                        .unwrap_or_default(),
                ),
            )?;
            write_duration(&mut w, duration)?;
            w.finish()?;
        }
        write!(w, "] ")?;

        cwrite!(bold(w), "{}", result.run())?;
        write!(w, "/")?;
        cwrite!(bold(w), "{}", result.expected())?;
        write!(w, " tests run: ")?;

        if result.passed() == result.total() {
            cwrite!(bold(w), "all {}", result.passed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Green), "passed")?;
        } else if result.failed() == result.total() {
            cwrite!(bold(w), "all {}", result.failed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Red), "failed")?;
        } else {
            cwrite!(bold(w), "{}", result.passed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Green), "passed")?;

            write!(w, ", ")?;
            cwrite!(bold(w), "{}", result.failed())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Red), "failed")?;
        }

        if result.filtered() != 0 {
            write!(w, ", ")?;
            cwrite!(bold(w), "{}", result.filtered())?;
            write!(w, " ")?;
            cwrite!(colored(w, Color::Yellow), "filtered")?;
        }

        writeln!(w)?;

        Ok(())
    }

    /// Report a test result and show supplementary information.
    pub fn report_test_result(&self, test: &Test, result: &TestResult) -> eyre::Result<()> {
        let (annot, color) = match result.stage() {
            Stage::Skipped => ("skip", Color::Yellow),
            Stage::Filtered => ("filter", Color::Yellow),
            Stage::FailedCompilation { .. } | Stage::FailedComparison(_) => ("fail", Color::Red),
            Stage::PassedCompilation => ("compile", Color::Green),
            Stage::PassedComparison => ("pass", Color::Green),
            Stage::Updated { .. } => ("update", Color::Green),
        };

        let mut w = ui::annotated(self.ui.stderr(), annot, color, RUN_ANNOT_PADDING)?;

        write!(w, "[")?;
        {
            let mut w = ui::colored(&mut w, duration_color(result.duration()))?;
            write_duration(&mut w, result.duration())?;
            w.finish()?;
        }
        write!(w, "] ")?;
        ui::write_test_id(&mut w, test.id())?;
        writeln!(w)?;

        ui::write_diagnostics(
            &mut w,
            self.ui.diagnostic_config(),
            self.world,
            result.warnings(),
            result.errors().unwrap_or_default(),
        )?;

        match result.stage() {
            Stage::PassedCompilation | Stage::PassedComparison => {}
            Stage::FailedCompilation { reference, .. } => {
                writeln!(
                    w,
                    "Compilation of {} failed",
                    if *reference { "reference" } else { "test" },
                )?;
            }
            Stage::FailedComparison(compare::Error {
                output,
                reference,
                pages,
            }) => {
                if output != reference {
                    writeln!(
                        w,
                        "Expected {reference} {}, got {output} {}",
                        Term::simple("page").with(*reference),
                        Term::simple("page").with(*output),
                    )?;
                } else {
                    for (p, e) in pages {
                        let p = p + 1;
                        match e {
                            PageError::Dimensions { output, reference } => {
                                writeln!(w, "Page {p} had different dimensions")?;
                                w.write_with(2, |w| {
                                    writeln!(w, "Output: {}", output)?;
                                    writeln!(w, "Reference: {}", reference)
                                })?;
                            }
                            PageError::SimpleDeviations { deviations } => {
                                writeln!(
                                    w,
                                    "Page {p} had {deviations} {}",
                                    Term::simple("deviation").with(*deviations),
                                )?;
                            }
                        }
                    }
                }
            }
            Stage::Updated { .. } => {}
            _ => unreachable!(),
        }

        Ok(())
    }
}

/// Writes a padded duration in human readable form
fn write_duration(w: &mut dyn Write, duration: TimeDelta) -> io::Result<()> {
    let s = duration.num_seconds();
    let ms = duration.subsec_millis();

    if s > 0 {
        write!(w, "{s: >2}s")?;
    } else {
        write!(w, "   ")?;
    }

    write!(w, " {ms: >3}ms")?;

    Ok(())
}

/// Returns the color to use for a test's duration.
fn duration_color(duration: TimeDelta) -> Color {
    match duration.num_seconds() {
        0 if duration.is_zero() => Color::Rgb(128, 128, 128),
        0 => Color::Green,
        1..=5 => Color::Yellow,
        _ => Color::Red,
    }
}
