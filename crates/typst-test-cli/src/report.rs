use std::fmt::{Debug, Display};
use std::io::Write;
use std::time::Duration;
use std::{fmt, io};

use semver::Version;
use termcolor::{Color, ColorSpec, HyperlinkSpec, WriteColor};
use typst_test_lib::compare;
use typst_test_lib::store::test::Test;

use crate::cli::OutputFormat;
use crate::project::Project;
use crate::test::{CompareFailure, TestFailure};
use crate::util;

pub const ANNOT_PADDING: usize = 8;

pub struct Summary {
    pub total: usize,
    pub filtered: usize,
    pub failed_compilation: usize,
    pub failed_comparison: usize,
    pub failed_otherwise: usize,
    pub passed: usize,
    pub time: Duration,
}

impl Summary {
    pub fn run(&self) -> usize {
        self.total - self.filtered
    }

    pub fn is_ok(&self) -> bool {
        self.passed == self.run()
    }

    pub fn is_partial_fail(&self) -> bool {
        self.passed < self.run()
    }

    pub fn is_total_fail(&self) -> bool {
        self.passed == 0
    }
}

fn write_with<W: WriteColor + ?Sized>(
    w: &mut W,
    set: impl FnOnce(&mut ColorSpec) -> &mut ColorSpec,
    unset: impl FnOnce(&mut ColorSpec) -> &mut ColorSpec,
    f: impl FnOnce(&mut W) -> io::Result<()>,
) -> io::Result<()> {
    w.set_color(set(&mut ColorSpec::new()))?;
    f(w)?;
    w.set_color(unset(&mut ColorSpec::new()))?;
    Ok(())
}

fn write_bold<W: WriteColor + ?Sized>(
    w: &mut W,
    f: impl FnOnce(&mut W) -> io::Result<()>,
) -> io::Result<()> {
    write_with(w, |c| c.set_bold(true), |c| c.set_bold(false), f)
}

fn write_bold_colored<W: WriteColor + ?Sized>(
    w: &mut W,
    annot: impl Display,
    color: Color,
) -> io::Result<()> {
    write_with(
        w,
        |c| c.set_bold(true).set_fg(Some(color)),
        |c| c.set_bold(false).set_fg(None),
        |w| write!(w, "{annot}"),
    )
}

pub struct Reporter {
    writer: Box<dyn WriteColor + Send + Sync + 'static>,

    // fmt::Write indenting fields
    indent: usize,
    need_indent: bool,
    spec: Option<ColorSpec>,

    // other confiuration
    format: OutputFormat,
}

impl Debug for Reporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reporter")
            .field("indent", &self.indent)
            .field("need_indent", &self.need_indent)
            .field("spec", &self.spec)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl Reporter {
    pub fn new<W: WriteColor + Send + Sync + 'static>(writer: W, format: OutputFormat) -> Self {
        Self {
            writer: Box::new(writer),
            indent: 0,
            need_indent: true,
            spec: None,
            format,
        }
    }

    pub fn with_indent<R>(&mut self, indent: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        if !self.format.is_pretty() {
            return f(self);
        }

        self.indent += indent;
        let res = f(self);
        self.indent -= indent;
        res
    }

    pub fn write_annotated(
        &mut self,
        annot: &str,
        color: Color,
        f: impl FnOnce(&mut Self) -> io::Result<()>,
    ) -> io::Result<()> {
        self.set_color(ColorSpec::new().set_bold(true).set_fg(Some(color)))?;
        if self.format.is_pretty() {
            write!(self, "{annot:>ANNOT_PADDING$} ")?;
        } else {
            write!(self, "{annot} ")?;
        }
        self.set_color(ColorSpec::new().set_bold(false).set_fg(None))?;
        self.with_indent(ANNOT_PADDING + 1, |this| f(this))?;
        Ok(())
    }

    pub fn warning(&mut self, warning: impl Display) -> io::Result<()> {
        self.write_annotated("warning:", Color::Yellow, |this| {
            writeln!(this, "{warning}")
        })
    }

    pub fn hint(&mut self, hint: impl Display) -> io::Result<()> {
        if !self.format.is_pretty() {
            return Ok(());
        }

        self.write_annotated("hint:", Color::Cyan, |this| writeln!(this, "{hint}"))
    }

    pub fn test_result(
        &mut self,
        test: &Test,
        annot: &str,
        color: Color,
        f: impl FnOnce(&mut Self) -> io::Result<()>,
    ) -> io::Result<()> {
        self.write_annotated(annot, color, |this| {
            write_bold(this, |w| writeln!(w, "{}", test.id()))?;
            f(this)
        })
    }

    pub fn test_progress(&mut self, test: &Test, annot: &str) -> io::Result<()> {
        self.test_result(test, annot, Color::Yellow, |_| Ok(()))?;
        Ok(())
    }

    pub fn test_success(&mut self, test: &Test, annot: &str) -> io::Result<()> {
        self.test_result(test, annot, Color::Green, |_| Ok(()))?;
        Ok(())
    }

    pub fn tests_success(&mut self, project: &Project, annot: &str) -> io::Result<()> {
        for test in project.matched().values() {
            self.test_success(test, annot)?;
        }

        Ok(())
    }

    pub fn tests_added(&mut self, project: &Project) -> io::Result<()> {
        self.tests_success(project, "added")?;
        Ok(())
    }

    pub fn test_added(&mut self, test: &Test) -> io::Result<()> {
        self.test_success(test, "added")?;
        Ok(())
    }

    pub fn test_failure(&mut self, test: &Test, error: TestFailure) -> io::Result<()> {
        self.test_result(test, "failed", Color::Red, |this| {
            if !this.format.is_pretty() {
                return Ok(());
            }

            match error {
                TestFailure::Compilation(e) => {
                    writeln!(
                        this,
                        "Compilation of {} failed",
                        if e.is_ref { "references" } else { "test" },
                    )?;

                    // TODO: proper span reporting reporting
                    writeln!(this, "{:#?}", e.error)?;
                }
                TestFailure::Comparison(CompareFailure::Visual {
                    error:
                        compare::Error {
                            output,
                            reference,
                            pages,
                        },
                    diff_dir,
                }) => {
                    if output != reference {
                        writeln!(
                            this,
                            "Expected {reference} {}, got {output} {}",
                            util::fmt::plural(reference, "page"),
                            util::fmt::plural(output, "page"),
                        )?;
                    }

                    for (p, e) in pages {
                        let p = p + 1;
                        match e {
                            compare::PageError::Dimensions { output, reference } => {
                                writeln!(this, "Page {p} had different dimensions")?;
                                this.with_indent(2, |this| {
                                    writeln!(this, "Output: {}", output)?;
                                    writeln!(this, "Reference: {}", reference)
                                })?;
                            }
                            compare::PageError::SimpleDeviations { deviations } => {
                                writeln!(
                                    this,
                                    "Page {p} had {deviations} {}",
                                    util::fmt::plural(deviations, "deviation",)
                                )?;
                            }
                        }
                    }

                    if let Some(diff_dir) = diff_dir {
                        this.hint(&format!(
                            "Diff images have been saved at '{}'",
                            diff_dir.display()
                        ))?;
                    }
                }
            }

            Ok(())
        })
    }

    pub fn package(&mut self, package: &str, version: Option<&Version>) -> io::Result<()> {
        write_bold_colored(self, package, Color::Cyan)?;
        if let Some(version) = version {
            write!(self, ":")?;
            write_bold_colored(self, version, Color::Cyan)?;
        }

        Ok(())
    }

    pub fn project(&mut self, project: &Project) -> io::Result<()> {
        struct Delims {
            open: &'static str,
            middle: &'static str,
            close: &'static str,
        }

        let (delims, align) = if self.format.is_pretty() {
            (
                Delims {
                    open: " ┌ ",
                    middle: " ├ ",
                    close: " └ ",
                },
                ["Template", "Project", "Tests"]
                    .map(str::len)
                    .into_iter()
                    .max()
                    .unwrap(),
            )
        } else {
            (
                Delims {
                    open: " ",
                    middle: " ",
                    close: " ",
                },
                0,
            )
        };

        if let Some(manifest) = project.manifest() {
            write!(self, "{:>align$}{}", "Project", delims.open)?;
            self.package(
                &manifest.package.name.to_string(),
                Some(&manifest.package.version),
            )?;
            writeln!(self)?;

            // TODO: list config settings + if it is manifest or file
            let _config = project.config();
        } else {
            write!(self, "{:>align$}{}", "Project", delims.open)?;
            write_bold_colored(self, "none", Color::Yellow)?;
            writeln!(self)?;
        }

        let tests = project.matched();
        write!(self, "{:>align$}{}", "Tests", delims.middle)?;
        if tests.is_empty() {
            write_bold_colored(self, "none", Color::Cyan)?;
            write!(self, " (searched at '{}')", project.tests_root().display())?;
        } else {
            write_bold_colored(self, tests.len(), Color::Cyan)?;
            write!(self, " (")?;
            write_bold_colored(
                self,
                tests.iter().filter(|(_, t)| t.is_ephemeral()).count(),
                Color::Yellow,
            )?;
            write!(self, " ephemeral)")?;
        }
        writeln!(self)?;

        write!(self, "{:>align$}{}", "Template", delims.close)?;
        match (project.template_path(), project.template()) {
            (None, None) => {
                write_bold_colored(self, "none", Color::Green)?;
            }
            (None, Some(_)) => {
                unreachable!("the path must be given for the file to be read");
            }
            (Some(path), None) => {
                write_bold_colored(self, "not found", Color::Red)?;
                write!(self, " (searched at '{}')", path.display())?;
            }
            (Some(_), Some(_)) => {
                write_bold_colored(self, "found", Color::Green)?;
            }
        }
        writeln!(self)?;

        Ok(())
    }

    pub fn test_start(&mut self, is_update: bool) -> io::Result<()> {
        if !self.format.is_pretty() {
            return Ok(());
        }

        write_bold(self, |w| {
            writeln!(
                w,
                "{} tests",
                if is_update { "Updating" } else { "Running" }
            )
        })
    }

    // TODO: the force option is not a pretty solution
    pub fn test_summary(
        &mut self,
        summary: Summary,
        is_update: bool,
        force: bool,
    ) -> io::Result<()> {
        if !self.format.is_pretty() && !force {
            return Ok(());
        }

        write_bold(self, |w| writeln!(w, "Summary"))?;
        self.with_indent(2, |this| {
            let color = if summary.is_ok() {
                Color::Green
            } else if summary.is_total_fail() {
                Color::Red
            } else {
                Color::Yellow
            };

            write_bold_colored(this, summary.passed, color)?;
            write!(this, " / ")?;
            write_bold(this, |w| write!(w, "{}", summary.run()))?;
            write!(this, " {}.", if is_update { "updated" } else { "passed" })?;

            if summary.failed_compilation != 0 {
                write!(this, " ")?;
                write_bold_colored(this, summary.failed_compilation, Color::Red)?;
                write!(this, " failed compilations.")?;
            }

            if summary.failed_comparison != 0 {
                write!(this, " ")?;
                write_bold_colored(this, summary.failed_comparison, Color::Red)?;
                write!(this, " failed comparisons.")?;
            }

            if summary.failed_otherwise != 0 {
                write!(this, " ")?;
                write_bold_colored(this, summary.failed_otherwise, Color::Red)?;
                write!(this, " failed otherwise.")?;
            }

            if summary.filtered != 0 {
                write!(this, " ")?;
                write_bold_colored(this, summary.filtered, Color::Yellow)?;
                write!(this, " filtered out.")?;
            }

            let secs = summary.time.as_secs();
            match (secs / 60, secs) {
                (0, 0) => writeln!(this),
                (0, s) => writeln!(
                    this,
                    " took {s} {}",
                    util::fmt::plural(s as usize, "second")
                ),
                (m, s) => writeln!(
                    this,
                    " took {m} {} {s} {}",
                    util::fmt::plural(m as usize, "minute"),
                    util::fmt::plural(s as usize, "second")
                ),
            }
        })
    }

    pub fn tests(&mut self, project: &Project) -> io::Result<()> {
        if self.format.is_pretty() {
            write_bold(self, |w| writeln!(w, "Tests"))?;
        }

        self.with_indent(2, |this| {
            for (name, test) in project.matched() {
                write!(this, "{name} ")?;
                if test.is_ephemeral() {
                    write_bold_colored(this, "ephemeral", Color::Yellow)?;
                } else {
                    write_bold_colored(this, "persistent", Color::Green)?;
                }
                writeln!(this)?;
            }

            Ok(())
        })
    }
}

impl fmt::Write for Reporter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_all(s.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl Write for Reporter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // NOTE: not being able to write fully to stdout/stderr would be an fatal in any case, this
        // greatly simplifies code used for indentation
        self.write_all(buf).map(|_| buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn write_all(&mut self, mut buf: &[u8]) -> io::Result<()> {
        let spec = self.spec.clone().unwrap_or_default();
        let pad = " ".repeat(self.indent);

        loop {
            if self.need_indent {
                match buf.iter().position(|&b| b != b'\n') {
                    None => break self.writer.write_all(buf),
                    Some(len) => {
                        let (head, tail) = buf.split_at(len);
                        self.writer.write_all(head)?;
                        self.writer.reset()?;
                        self.writer.write_all(pad.as_bytes())?;
                        self.writer.set_color(&spec)?;
                        self.need_indent = false;
                        buf = tail;
                    }
                }
            } else {
                match buf.iter().position(|&b| b == b'\n') {
                    None => break self.writer.write_all(buf),
                    Some(len) => {
                        let (head, tail) = buf.split_at(len + 1);
                        self.writer.write_all(head)?;
                        self.need_indent = true;
                        buf = tail;
                    }
                }
            }
        }
    }
}

impl WriteColor for Reporter {
    fn supports_color(&self) -> bool {
        self.writer.supports_color()
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        self.spec = Some(spec.clone());
        self.writer.set_color(spec)
    }

    fn reset(&mut self) -> io::Result<()> {
        self.spec = None;
        self.writer.reset()
    }

    fn is_synchronous(&self) -> bool {
        self.writer.is_synchronous()
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        self.writer.set_hyperlink(link)
    }

    fn supports_hyperlinks(&self) -> bool {
        self.writer.supports_hyperlinks()
    }
}
