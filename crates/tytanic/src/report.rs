//! Live reporting of test progress.

use std::io::{self, Write};
use std::time::Duration;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::doc::compare::{self, PageError};
use tytanic_core::suite::SuiteResult;
use tytanic_core::test::{Stage, Test, TestResult};
use tytanic_utils::fmt::Term;

use crate::cwrite;
use crate::ui::{self, CWrite, Ui};
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
                        .checked_div(result.run() as u32)
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

        // TODO(tinger): report failures, mean and avg time

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

        let duration = result.timestamp().elapsed();

        let mut w = ui::annotated(self.ui.stderr(), "", Color::Black, RUN_ANNOT_PADDING)?;

        write!(w, "[")?;
        {
            let mut w = ui::colored(
                &mut w,
                duration_color(
                    duration
                        .checked_div(result.run() as u32)
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
                    let pad = usize::try_from(pages.len().ilog10() + 1)
                        .expect("the page count cannot be higher than usize::MAX");

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
                                let count: usize = deviations.values().copied().sum();

                                #[allow(clippy::erasing_op, clippy::identity_op)]
                                let bins = [
                                    // char, threshold
                                    (" ", 0 * 255 * 3 / 8),
                                    ("▁", 1 * 255 * 3 / 8),
                                    ("▂", 2 * 255 * 3 / 8),
                                    ("▃", 3 * 255 * 3 / 8),
                                    ("▄", 4 * 255 * 3 / 8),
                                    ("▅", 5 * 255 * 3 / 8),
                                    ("▆", 6 * 255 * 3 / 8),
                                    ("▇", 7 * 255 * 3 / 8),
                                    ("█", 8 * 255 * 3 / 8),
                                ];

                                let mut histogram = String::new();
                                for (lo, hi) in bins.windows(2).map(|s| (s[0].1, s[1].1)) {
                                    let bin_count: usize =
                                        deviations.range(lo..=hi).map(|(_, &c)| c).sum();

                                    let idx = bins
                                        .binary_search_by_key(&(count / bin_count), |&(_, bin)| {
                                            usize::from(bin)
                                        })
                                        .unwrap_or_else(|e| e);

                                    histogram.push_str(bins[idx].0);
                                }

                                writeln!(w, "Page {p: >pad$} [{histogram}] total: {count}")?;
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
fn write_duration(w: &mut dyn Write, duration: Duration) -> io::Result<()> {
    let s = duration.as_secs();
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
fn duration_color(duration: Duration) -> Color {
    match duration.as_secs() {
        0 if duration.is_zero() => Color::Rgb(128, 128, 128),
        0 => Color::Green,
        1..=5 => Color::Yellow,
        _ => Color::Red,
    }
}
